#!/usr/bin/env python3
import os
import sys
import shutil
import subprocess
import re

# Standard libraries that should be provided by the host system and NOT bundled.
EXCLUDE_PREFIXES = (
    # Core C library and dynamic linker
    "ld-linux",
    "libc.so",
    "libdl.so",
    "libm.so",
    "libmvec.so",
    "libpthread.so",
    "libresolv.so",
    "librt.so",
    "libutil.so",
    "libanl.so",
    "libBrokenLocale.so",
    "libcidn.so",
    "libnss_",
    "libthread_db.so",
    # Compiler runtimes (use host)
    "libstdc++.so",
    "libgcc_s.so",
    # Graphics drivers & core display protocol libraries (MUST match host Mesa / kernel)
    "libGL.so",
    "libEGL.so",
    "libGLdispatch.so",
    "libGLX.so",
    "libOpenGL.so",
    "libdrm.so",
    "libglapi.so",
    "libgbm.so",
    "libxcb.so",
    "libX11.so",
    "libX11-xcb.so",
    "libXau.so",
    "libXdmcp.so",
    "libwayland-client.so",
    # Core audio base
    "libasound.so",
    # Host font configuration
    "libfontconfig.so",
    "libfreetype.so",
    "libharfbuzz.so",
    # Core system identifiers
    "libuuid.so",
    "libz.so",
)

def is_excluded(soname):
    base = os.path.basename(soname)
    for p in EXCLUDE_PREFIXES:
        if base == p or base.startswith(p + ".") or (p.endswith(".so") and base.startswith(p)):
            return True
        if not p.endswith(".so") and base.startswith(p):
            return True
    return False

def get_deps(file_path):
    deps = {}
    try:
        out = subprocess.check_output(["ldd", file_path], text=True, stderr=subprocess.DEVNULL)
    except Exception:
        return deps
    for line in out.splitlines():
        line = line.strip()
        match = re.match(r"^(.*?)\s*=>\s*(.*?)\s*\(0x[0-9a-fA-F]+\)$", line)
        if match:
            soname = match.group(1).strip()
            path = match.group(2).strip()
            if path and os.path.isabs(path) and os.path.exists(path):
                deps[soname] = path
    return deps

def main():
    if len(sys.argv) < 2:
        print("Usage: bundle_appimage_deps.py <AppDir>", file=sys.stderr)
        sys.exit(1)

    appdir = os.path.abspath(sys.argv[1])
    lib_dir = os.path.join(appdir, "usr", "lib")
    os.makedirs(lib_dir, exist_ok=True)

    print(f"Bundling AppImage dependencies into {lib_dir}...")

    # 1. Copy WebKitGTK processes if available
    webkit_search_dirs = [
        "/usr/lib/webkitgtk-6.0",
        "/usr/lib/x86_64-linux-gnu/webkitgtk-6.0",
        "/usr/libexec/webkitgtk-6.0",
    ]
    for src in webkit_search_dirs:
        if os.path.isdir(src):
            dst = os.path.join(lib_dir, "webkitgtk-6.0")
            os.makedirs(dst, exist_ok=True)
            for item in os.listdir(src):
                s = os.path.join(src, item)
                d = os.path.join(dst, item)
                if os.path.isfile(s) and not os.path.exists(d):
                    shutil.copy2(s, d)
            print(f"Copied WebKit processes from {src} to {dst}")
            break

    # 2. Collect initial binaries to scan
    to_scan = []
    bin_dir = os.path.join(appdir, "usr", "bin")
    if os.path.isdir(bin_dir):
        for f in os.listdir(bin_dir):
            p = os.path.join(bin_dir, f)
            if os.path.isfile(p) and os.access(p, os.X_OK):
                to_scan.append(p)

    webkit_dst = os.path.join(lib_dir, "webkitgtk-6.0")
    if os.path.isdir(webkit_dst):
        for f in os.listdir(webkit_dst):
            p = os.path.join(webkit_dst, f)
            if os.path.isfile(p) and os.access(p, os.X_OK):
                to_scan.append(p)

    # 3. Recursively copy shared libraries
    scanned = set()
    copied_count = 0

    while to_scan:
        curr = to_scan.pop(0)
        real_curr = os.path.realpath(curr)
        if real_curr in scanned:
            continue
        scanned.add(real_curr)

        deps = get_deps(curr)
        for soname, path in deps.items():
            if is_excluded(soname):
                continue

            real_path = os.path.realpath(path)
            real_name = os.path.basename(real_path)
            real_dest = os.path.join(lib_dir, real_name)

            if not os.path.exists(real_dest):
                try:
                    shutil.copy2(real_path, real_dest)
                    os.chmod(real_dest, 0o755)
                    copied_count += 1
                    to_scan.append(real_path)
                except Exception as e:
                    print(f"Warning: failed to copy {real_path} to {real_dest}: {e}", file=sys.stderr)

            # Ensure soname link exists
            soname_dest = os.path.join(lib_dir, soname)
            if not os.path.exists(soname_dest):
                try:
                    os.symlink(real_name, soname_dest)
                except OSError:
                    try:
                        shutil.copy2(real_path, soname_dest)
                    except Exception:
                        pass

            # If the path basename differed from soname and realname, create link for it as well
            path_base = os.path.basename(path)
            path_dest = os.path.join(lib_dir, path_base)
            if not os.path.exists(path_dest):
                try:
                    os.symlink(real_name, path_dest)
                except OSError:
                    pass

    print(f"Bundled {copied_count} shared libraries.")

    # 4. Copy and compile GSettings schemas
    schemas_dst = os.path.join(appdir, "usr", "share", "glib-2.0", "schemas")
    os.makedirs(schemas_dst, exist_ok=True)
    schemas_src = "/usr/share/glib-2.0/schemas"
    if os.path.isdir(schemas_src):
        for f in os.listdir(schemas_src):
            if f.endswith(".gschema.xml") or f.endswith(".override"):
                s = os.path.join(schemas_src, f)
                d = os.path.join(schemas_dst, f)
                if not os.path.exists(d):
                    shutil.copy2(s, d)

        # Compile schemas
        try:
            subprocess.run(["glib-compile-schemas", schemas_dst], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print("Compiled GSettings schemas.")
        except Exception as e:
            print(f"Warning: could not compile schemas in {schemas_dst}: {e}", file=sys.stderr)

    print("Dependency bundling completed successfully.")

if __name__ == "__main__":
    main()
