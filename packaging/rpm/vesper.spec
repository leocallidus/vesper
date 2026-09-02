Name:           vesper
Version:        1.2.1
Release:        1%{?dist}
Summary:        Linux desktop screensaver written in Rust with GTK4

# Local packaging: don't emit debuginfo/debugsource subpackages.
# (This project builds a release binary without DWARF by default, and rpm 6 errors
# out if the generated debugsource file list is empty.)
%global debug_package %{nil}

License:        MIT
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  pkgconfig(webkitgtk-6.0)

%description
Vesper is a Linux desktop screensaver written in Rust with GTK4.

%prep
%autosetup -n %{name}-%{version}

%build
export CARGO_HOME="%{_builddir}/cargo-home"

# gtk4 (and friends) can make rustc/LLVM use a lot of RAM with distro-injected
# flags like `-Ccodegen-units=1`. For local builds, prefer lower peak memory.
unset CARGO_ENCODED_RUSTFLAGS
unset RUSTFLAGS
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

# Prefer an offline/reproducible build when the source tarball includes vendored crates.
if [ -d vendor ]; then
  mkdir -p .cargo
  if [ ! -f .cargo/config.toml ]; then
    cat > .cargo/config.toml << 'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF
  fi
  export CARGO_NET_OFFLINE=true
  cargo build --release --locked --offline -j 1
else
  cargo build --release --locked -j 1
fi

%install
install -Dpm0755 target/release/vesper %{buildroot}%{_bindir}/vesper

install -Dpm0644 packaging/rpm/com.example.vesper.desktop \
  %{buildroot}%{_datadir}/applications/com.example.vesper.desktop

install -Dpm0644 packaging/appimage/vesper.svg \
  %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/vesper.svg

%files
%license LICENSE
%doc README.md README_ru.md docs
%{_bindir}/vesper
%{_datadir}/applications/com.example.vesper.desktop
%{_datadir}/icons/hicolor/scalable/apps/vesper.svg

%changelog
* Sun Jan 25 2026 Codex <codex@localhost> - 1.1.0-1
- Initial RPM packaging
