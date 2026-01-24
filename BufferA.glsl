/*
 *
 *   Warped Offset Block Corridor
 *   ----------------------------
 *
 *   I coded this up a while ago. It's a little on the quirky side, but I
 *   liked the way it turned out. It's pieced together from other examples
 *   of mine. The object of the exercise was to create a simple scene that
 *   gave the illusion of reflective and refractive materials without the
 *   added difficulty and exorbitant cost.
 *
 *   For anyone curious, the geometry itself was produced via a traveral of
 *   offset blocks that have been interwoven along the floor and wall joins.
 *   It differs from a regular voxel traversal in the sense that the
 *   individual block columns and rows have been offset from one another.
 *
 *   As quick demonstrations go, this is fine. However, there's a lot of
 *   scene and lighting hackery involved, so it's probably not the best
 *   example to learn from. The code works fine and the logic is OK, but it
 *   was pieced together without an overall plan, so it could do with some
 *   attention. The character count could also be improved upon.
 *
 *
 *
 *   Other examples:
 *
 *   // I love the way this one is presented.
 *   pow(The Shining, 2.0) -- dean_the_coder
 *   https://www.shadertoy.com/view/tlyfDV
 *
 *   // Diatribes puts together a lot of interesting tunnel examples.
 *   // I like the motion, color and lowish character count of this one.
 *   Orbs of the Gyroid -- diatribes
 *   https://www.shadertoy.com/view/3fKyz3
 *
 */

////////////////////
// PI and 2 PI.
#define PI 3.14159265
#define TAU 6.2831853
// Far plane.
#define FAR 40.

// Loop... anti-unrolling hack. :)
#define ZERO min(iFrame, 0)

/////////////////////

// Tri-Planar blending function: Based on an old Nvidia writeup:
// GPU Gems 3 - Ryan Geiss: http://http.developer.nvidia.com/GPUGems3/gpugems3_ch01.html
vec3 tex3D(sampler2D tex, in vec3 p, in vec3 n){

    // Abosolute normal with a bit of tightning.
    n = max(n*n - .2, .001); // max(abs(n), 0.001), etc.
    n /= dot(n, vec3(1));
    //n /= length(n);

    // Texure samples. One for each plane.
    vec3 tx = texture(tex, p.zy).xyz;
    vec3 ty = texture(tex, p.xz).xyz;
    vec3 tz = texture(tex, p.xy).xyz;

    // Multiply each texture plane by its normal dominance factor.... or however you wish
    // to describe it. For instance, if the normal faces up or down, the "ty" texture
    // sample, represnting the XZ plane, will be used, which makes sense.

    // Textures are stored in sRGB (I think), so you have to convert them to linear space
    // (squaring is a rough approximation) prior to working with them... or something like
    // that. :) Once the final color value is gamma corrected, you should see correct
    // looking colors.
    return mat3(tx*tx, ty*ty, tz*tz)*n;
}


// Commutative smooth maximum function. Provided by Tomkh, and taken
// from Alex Evans's (aka Statix) talk:
// http://media.lolrus.mediamolecule.com/AlexEvans_SIGGRAPH-2015.pdf
// Credited to Dave Smith @media molecule.
float smax(float a, float b, float k){

    float f = max(0., 1. - abs(b - a)/k);
    return max(a, b) + k*.25*f*f;
}


// The path is a 2D sinusoid that varies over time, depending upon the frequencies,
// and amplitudes.
vec2 path(in float z){

    return vec2(0); // Straight line.

    /*
     *   // This example doesn't quite work with a curved path, but I have
     *   // one that does, so I'll update it later.
     *   // Curved path.
     *   float a = sin(z*.11);
     *   float b = cos(z*.14);
     *   return vec2(a*4. - b*1.5, b*1.7 + a*1.5);
     */
}

// IQ's 2D box function, with added smoothing factor.
float sBoxS(in vec2 p, in vec2 b, in float rf){

    vec2 d = abs(p) - b + rf;
    return min(max(d.x, d.y), 0.) + length(max(d, 0.)) - rf;
}


// IQ's 3D signed box formula: I tried saving calculations by using the unsigned one, and
// couldn't figure out why the edges and a few other things weren't working. It was because
// functions that rely on signs require signed distance fields... Who would have guessed? :D
float sBoxS(vec3 p, vec3 b, float sf){

    p = abs(p) - b + sf;
    return min(max(p.x, max(p.y, p.z)), 0.) + length(max(p, 0.)) - sf;
}


// Surface ID and glow.
int gID;
vec3 glow;

// The voxel isosurface function.
float getFunc(vec3 p){

    // Offset the tunnel about the XY plane as we traverse Z.
    // The path is straight, so this is redundant here, but I'll add
    // the functionality in later.
    p.xy -= path(p.z);

    // Mulitples of the scale, 1.5.

    // Square tunnel.
    return -max(abs(p.y) - 3., abs(p.x) - 4.5);

    /*
     *   // Interesting, but we're keeping it simple.
     *   vec3 qq = p;
     *   qq.xy *= rot2(qq.z*.15);
     *   return -max(abs(qq.y) - 3., abs(p.x) - 4.5) + 1.5;
     */
}



// Object color.
vec3 getCol(vec3 p){

    float range = hash31(p + .011)/6.; // 0 to 1.
    float saturation = .7;//hash31(p + .031)*.2 + .6; // 0 to 1.
    return .5 + .45*cos(TAU*range + vec3(0, 1.57, 3.14)*saturation - .5);

}

// Voxel column or row offset. Voxel example don't commly contain these, but
// it's possible.
float posOffs(vec3 p){


    //return 0.; // No offset.
    //return (hash31(p + .1) - .5); // Random.

    // Rough sinusoidal layer noise.
    float ns = dot(sin(p - cos(p.yzx*1.5 + TAU/3.)*TAU/3.), vec3(1)/6.);
    p = p*1.5 + TAU/2.;
    return mix(ns, dot(sin(p - cos(p.yzx*1.5 + TAU/3.)*TAU/3.), vec3(1)/6.), .35);

}


// Subdivided rectangle grid.
vec3 getGrid(vec3 p, inout vec3 sc, inout vec3 id){


    // Block offsets.
    vec3 ipOffs = vec3(0);

    // X, Y and Z offset ID.
    vec3 ii = floor(p/sc) + .5;
    vec3 mp = mod(ii - .5, 2.);

    int edge = 0;

    // Floor offsets.
    if(abs(ii.x)<=2.5 && abs(ii.y)>=1.5) {


        float rndY = posOffs(vec3(ii.x, .137, ii.z));//2./3.;

        // Set alternate Z on the X-edges to zero.
        if(abs(ii.x)==2.5 && mp.z<.5){ rndY = 0.; edge = 1; }

        p.y -= rndY*sc.y; // Row offset.
        ipOffs.y += rndY;

    }

    // Side wall offsets.
    if(abs(ii.y)<=1.5 && abs(ii.x)>=2.5) {


        float rndX = posOffs(vec3(.34, ii.yz));//2./3.;//

        // Set alternate Z on the X-edges to zero.
        if(abs(ii.y)==1.5 && mp.z>.5){ rndX = 0.; edge = 1; }

        p.x -= rndX*sc.x; // Row offset.
        ipOffs.x += rndX;


    }



    // Original position.
    vec3 oP = p;


    // Block ID.
    vec3 ip = floor(p/sc) + .5;

    /*
     *   // Subdivision. It'll work, but I have better working examples
     *   // that I'll post later.
     *   #define SUBDIV
     *
     *   #ifdef SUBDIV
     *   //#define EQUAL_SIDES
     *
     *   // Subdivide.
     *   for(int i = 0; i<1; i++){
     *
     *       //if(edge==1) break;
     *       // Current block ID.
     *       //p = oP;
     *
     *       float fi = float(i)*.0617; // Unique loop number.
     *
     *       #ifdef EQUAL_SIDES
     *       // Squares.
     *
     *       sc /= 2. - step(.333, hash31(ip + .253 + fi));
     *
     *       #else
     *
     *       // Powers of two rectangles.
     *
     *       vec3 h33 = hash33(ip + .253 + fi);
     *       // h42 = texture(iChannel2, ip*sc*113.619 + .253 + fi);
     *
     *
     *       // Divide by 2, or 1 (do nothing), if the random threshold
     *       // is met in any direction.
     *       sc /= 2. - step(.333, h33);
     *
     *       #endif
     *
     *
     *       ip = floor(p/sc) + .5;
     *
}

#endif

*/


    // Cell ID (id is an "inout" variable).
    id = (ip + ipOffs)*sc;

    // Return the local coordinates.
    return p - ip*sc;

}

// Global cell boundary distance variables.
vec3 gDir; // Cell traversing direction.
vec3 gRd; // Ray direction.
float gCD; // Cell boundary distance.
// Box dimension and local XY coordinates.
vec3 gSc;
vec3 gP;

// General global container. It's used to save things in the map function
// for later usage.
vec4 gVal;


// Window and frame flag.
int gFrame;
// Full voxel value.
float oVx;

///////////////////
float getVox(vec3 p){


    // The voxel grid.
    vec3 sc = vec3(1.5), ip; // Scale and ID.
    p = getGrid(p, sc, ip); // The grid object.

    // Saving the block ID for later use..
    gVal = vec4(0, ip);


    // The voxel function. This one is just a standard square tunnel.
    float fn = getFunc(ip);

    // Voxel object.
    float vox = 1e5;

    // Window frame flag.
    gFrame = -1;

    // If the voxel surface function is under the threshold, render an object.
    if(fn<0.){

        // Slightly rounded box.
        float minSc = min(min(sc.x, sc.y), sc.z);
        float gap = .0215;
        float smF = .05*minSc;
        vox = sBoxS(p, sc/2. - gap, smF);

        oVx = vox; // Full box.

        // Frame and window variable.
        gFrame = 0;

        // Cut out some windows from the 1st and 2nd rows.
        if(abs(floor(ip.y/1.5) + .5) <= 1. && abs(floor(ip.x/1.5))<=4.){
            //if(abs(ip.y) <= 2.25 && mod(floor(ip.z/1.5) + floor(ip.y/1.5), 2.)==0.){

            //vec3 rnd3 = hash33(ip + .04);
            float ew = .2;

            //if(rnd3.x<.65){
            float voxX = sBoxS(p.yz, sc.yz/2. - ew, smF/2.);
            //voxX = smax(voxX, sign(q.x)*(p.x + (sc.x/2. - ew)), smF/2.);
            vox = smax(vox, -voxX, .02);

            // Flag that a frame and window object should be added.
            gFrame = 1;
            // }

        }

    }

    // Saving the local voxel coordinates and scale. The scale is fixed
    // here, but it can change.
    gP = p;
    gSc = sc;

    // Voxel object.
    return vox;

}




// Perturbed gyroid (or cellular) tunnel function: In essence, it's one or two
// smoothly combined gyroid functions, with a cylindrical hole (wrapped around the
// camera path) smoothly carved out from them.
//
float map(vec3 p){



    // Trancendental gyroid or cellular functions with extra functions to perturb
    // the tunnel.
    //
    // As and aside, I hadn't looked at this example for ages, so I had to spend
    // a while trying to figure out what all this mess was... And that is why you
    // should comment as you go. :D

    vec3 oP = p;


    // Cube voxel distance.
    float vx = getVox(oP);


    ////////////////////

    // We need to cover both directions, so we take the absolute value.
    vec3 rC = abs((gDir*gSc - gP)/gRd);
    //vec2 rC = (gDir.xy*gSc.xy - gP)/gRd.xy; // For 2D, this will work too.

    // Minimum of all distances, plus not allowing negative distances, which
    // stops the ray from tracing backwards... I'm not entirely sure it's
    // necessary here, but it stops artifacts from appearing with other
    // non-rectangular grids.
    gCD = max(min(min(rC.x, rC.y), rC.z), 0.) + .0001;
    //gCD = max(min(rC.x, rC.y), 0.) + .001; // Adding a touch to advance to the next cell.

    ////////////////////

    // Hacky last minute logic to accomodate for windows and frames.
    float window = 1e5;
    float frame = 1e5;

    // If we're at the right Y position, construct window frames and
    // the windows themselves.
    if(gFrame == 1){
        window = oVx + .2;
        frame = max(vx, -(window - .002));
        window = abs(window + .01) - .01;
        vx = 1e5;
    }

    // When near the vacinity of the glass, add some glow.
    if(window<.1){

        // Random block glow intensity.
        vec3 h33 = hash33(gVal.yzx + .41);

        // Light position.
        vec3 pL = (gP - vec3(0, gSc.y/2. - .1, 0)*0.)/gSc;
        float lObj = dot(pL, pL); // Squared distance from light.
        vec3 gCol = getCol(gVal.yzw); // Block glow color.
        // Adding the glow.
        glow += (h33.z*.7 + .3)*(gCol*.9 + .1)/(.001 + lObj);
    }

    // Object ID.
    gID = vx<frame && vx<window ? 0 : frame<window? 1 : 2;


    // Return the distance value for the scene.
    return min(vx, min(frame, window));

}


// Standard raymarching function.
float trace(in vec3 ro, in vec3 rd){

    // Reset the glow to zero.
    glow = vec3(0);

    // Set the global ray direction varibles -- Used to calculate
    // the cell boundary distance inside the "map" function.
    gDir = step(0., rd) - .5;
    gRd = rd;

    // Note the jittering, since we're using cheap glow.
    float d, t = hash31(fract(ro*89.567)*7. + rd)*.25;
    for(int i = ZERO; i<80; i++){

        // Surface distance.
        d = map(ro + rd*t);
        // Surface distance check.
        if(abs(d)<.001 || t>FAR) break; // Alternative: 0.001*max(t*.25, 1.)
        // Since we're calculatig glow inside the distance function (which is
        // a cheap hack), we need to delimit the ray jumping distance a bit.
        t += min(d*.7, gCD);//min(d*.9, .2);

    }

    // Clamp the distace to the far plane, in order to avoid occasional artifacts.
    return min(t, FAR);
}

// Normal function. It's not as fast as the tetrahedral calculation, but more symmetrical.
vec3 normal(in vec3 p) {

    //return normalize(vec3(m(p + e.xyy) - m(p - e.xyy), m(p + e.yxy) - m(p - e.yxy),
    //                      m(p + e.yyx) - m(p - e.yyx)));

    // This mess is an attempt to speed up compiler time by contriving a break... It's
    // based on a suggestion by IQ. I think it works, but I really couldn't say for sure.
    float sgn = 1.;
    vec3 e = vec3(.001, 0, 0), mp = e.zzz; // Spalmer's clever zeroing.
    for(int i = ZERO; i<6; i++){
        mp.x += map(p + sgn*e)*sgn;
        sgn = -sgn;
        if((i&1)==1){ mp = mp.yzx; e = e.zxy; }
    }

    return normalize(mp);
}


// Cheap shadows are hard. In fact, I'd almost say, shadowing particular scenes with
// limited iterations is impossible... However, I'd be very grateful if someone could
// prove me wrong. :)
float softShadow(vec3 ro, vec3 rd, vec3 n, float lDist, float k){


    // Coincides with the hit condition in the "trace" function.
    ro += n*.0015;


    float shade = 1.;
    float t = 0.;


    // I've added in a touch of jittering to alleviate banding.
    ro += rd*hash31(ro + n*57.13)*.01;


    // Set the global ray direction varibles -- Used to calculate
    // the cell boundary distance inside the "map" function.
    gDir = step(0., rd) - .5;
    gRd = rd;


    // Max shadow iterations - More iterations make nicer shadows, but slow things down.
    // Obviously, the lowest number to give a decent shadow is the best one to choose.
    for (int i = min(0, iFrame); i<80; i++){

        float d = map(ro + rd*t);
        shade = min(shade, k*d/t);
        // shade = min(shade, smoothstep(0., 1., k*d/t)); // Thanks to IQ for this tidbit.

        // Early exits from accumulative distance function calls tend to be a good thing.
        if (d<0. || t>lDist) break;


        // So many options here, and none are perfect:
        // dist += clamp(d, .01, stepDist), etc.
        t += clamp(min(d, gCD), .01, .15);

    }

    // Shadow.
    return max(shade, 0.);
}


// I keep a collection of occlusion routines... OK, that sounded really nerdy. :)
// Anyway, I like this one. I'm assuming it's based on IQ's original.

// For anyone not familiar with the process, the idea of the function is to very
// roughly approximate the self shadowing that occurs around a surface when light
// is being bounced all over the place. In particular, it marches out from the
// surface in the direction of the surface normal, then determines the overall light
// occlusion based on how far the ray is from any given surface. It also factors in
// how far away the ray is from orginating surface point itself. You can see all that
// in the workings.
float calcAO(in vec3 p, in vec3 n){

    float sca = 2., occ = 0.;
    for( int i = 0; i<5; i++ ){

        float hr = float(i + 1)*.15/5.;
        float d = map(p + n*hr);
        occ += (hr - d)*sca;
        sca *= .75;
    }

    return clamp(1. - occ, 0., 1.);
}


// Global object ID for the bump function.
// Hacky, but quick.
int bGID = 0;

// Surface bump function..
float bumpSurf3D(in vec3 q, in vec3 n){

    float vx = getVox(q);

    vec3 cM = cubeMap(gP, gSc);
    vec2 uv = cM.xy;
    float faceID = cM.z;
    vec2 scF = faceID<1.5? gSc.zy : faceID<3.5? gSc.xz : gSc.xy;

    // Box panel.
    float d = sBoxS(uv, scF/2., 0.);

    if(bGID==1) d = max(d + .04, -(d + .2));
    else d += .04;

    //d = smoothstep(0., .04, -d);
    //d = max(d, -abs(panel + .2) - .05);

    d = .75 - d*.25;


    //d = smoothstep(0., .04, -d)*.75 - d*.25;

    return d;

}

// Standard function-based bump mapping routine: This is the cheaper four tap version.
// There's a six tap version (samples taken from either side of each axis), but this
// works well enough.
vec3 doBumpMap(in vec3 p, in vec3 n, float bumpfactor){


    // Larger sample distances give a less defined bump, but can sometimes lessen the
    // aliasing.
    const vec2 e = vec2(.001, 0);

    // Sample positions.
    mat4x3 p4 = mat4x3(p, p - e.xyy, p - e.yxy, p - e.yyx);

    // This utter mess is to avoid longer compile times. It's kind of
    // annoying that the compiler can't figure out that it shouldn't
    // unroll loops containing large blocks of code.

    vec4 b4;
    for(int i = 0; i<4; i++){
        b4[i] = bumpSurf3D(p4[i], n);
        if(n.x>1e5) break; // Fake break to trick the compiler.
    }

    // Gradient vector: vec3(df/dx, df/dy, df/dz);
    vec3 grad = (b4.yzw - b4.x)/e.x;


    // Six tap version, for comparisson. No discernible visual difference, in a lot of
    //cases.
    //vec3 grad = vec3(bumpSurf3D(p - e.xyy) - bumpSurf3D(p + e.xyy),
    //                 bumpSurf3D(p - e.yxy) - bumpSurf3D(p + e.yxy),
    //                 bumpSurf3D(p - e.yyx) - bumpSurf3D(p + e.yyx))/e.x*.5;


    // Adjusting the tangent vector so that it's perpendicular to the normal. It's some
    // kind of orthogonal space fix using the Gram-Schmidt process, or something to that
    // effect.
    grad -= n*dot(n, grad);

    // Applying the gradient vector to the normal. Larger bump factors make things more
    // bumpy.
    return normalize(n + grad*bumpfactor);

}

// Planar to spherical camera. Not quite, but close enough.
vec3 sphereCam(in vec2 p){

    //return normalize(vec3(p, 1)); // Debug.

    float t = 1./(1. + dot(p,p)/3.);
    return vec3(p*t, 2.*t - 1.);
}


void mainImage( out vec4 fragColor, in vec2 fragCoord ){

    // Screen coordinates.
    vec2 uv = (fragCoord - iResolution.xy*.5)/iResolution.y;

    // Extra screen bulge. Not needed here.
    //uv *= .8 + dot(uv, uv)*.4;

    // Camera Setup.
    vec3 lookAt = vec3(0, 0, iTime*4.);  // "Look At" position.
    vec3 camPos = lookAt + vec3(0, 0, -.2); // Camera position, doubling as the ray origin.

    // Light positioning.
    vec3 lightPos = camPos + vec3(0, 1, 5); // Placed in front of the camera.

    // Using the Z-value to perturb the XY-plane.
    lookAt.xy += path(lookAt.z);
    camPos.xy += path(camPos.z);
    lightPos.xy += path(lightPos.z);

    // Using the above to produce the unit ray-direction vector.
    float FOV = TAU/6.; // FOV - Field of view.
    vec3 forward = normalize(lookAt - camPos);
    vec3 right = normalize(vec3(forward.z, 0, -forward.x ));
    vec3 up = cross(forward, right);

    // rd - Ray direction.
    //vec3 rd = normalize(uv.x*right + uv.y*up + forward/FOV );
    mat3 cam = mat3(right, up, forward);
    //vec3 rd = cam*normalize(vec3(uv, 1./FOV));
    // A bit of lens mutation to increase the scene peripheral, if that's your thing.
    vec3 rd = cam*sphereCam(uv*PI*.7/FOV);


    // Swiveling the camera about the XY-plane (from left to right) when turning corners.
    // Naturally, it's synchronized with the path in some kind of way.
    rd.xy = rot2(-path(lookAt.z).x/16.)*rd.xy;

    // Rotating a little further to the desired camera angle.
    rd.xy *= rot2(atan(iResolution.y, iResolution.x) - .12);
    rd.xz *= rot2(PI/2.85);

    // Mouse movement.
    if(iMouse.z>1.){
        rd.yz *= rot2((iMouse.y - iResolution.y*.5)/iResolution.y*3.1459);
        rd.xz *= rot2((iMouse.x - iResolution.x*.5)/iResolution.x*3.1459);
    }


    // Standard ray marching routine.
    float t = trace(camPos, rd);

    // Object ID and glow.
    int svGID = gID;
    vec3 svGlow = glow;
    vec4 svVal = gVal;


    // Object ID for the bump function.
    bGID = gID;


    // Surface position.
    vec3 sp = camPos + t*rd;
    vec2 pth = sp.xy - path(sp.z);


    // Sky, or background light, in this case.
    vec3 sky = vec3(1, .65, .35);

    // Fake background sky coloring. Light in the horizon center and
    // darker on the outsides.
    sky = mix(sky*sky, sky*1.25, clamp(-sBoxS(pth.xy, vec2(5, 3), .5), 0., 1.));
    //sky = mix(sky, sky.zyx, smoothstep(-5., 5., pth.x - 3.5));

    // Initialize the scene color.
    vec3 sceneCol = sky;



    // The ray has effectively hit the surface, so light it up.
    if(t<FAR){


        // Normal direction.
        float nDir = 1.;

        // This gives the illusion of a windowed surface, which is fine
        // for the purpose of this example.
        if(svGID==2){

            // Distance from the window pane to the inner part of
            // the room (block it's attached to).
            // Neglidgible refraction through a thin window, so
            // we won't bother.

            // Using raytracing (box) methods to obtain the distance
            // to the glass block exit.
            vec3 rC3 = abs((gDir*max(gSc - .2*2., 0.) - gP)/rd);
            float dst = max(min(min(rC3.x, rC3.y), rC3.z), 0.);

            // Surface hit point.
            sp = sp + rd*dst;
            // Edging out a bit to avoid self-collisions.
            sp -= .002*rd;

            // This hack related to the fact that we're technically still
            // on the inside of the glass, so the normal needs to be
            // reversed to face us.
            nDir *= -1.;

        }

        // Surface normal.
        vec3 sn = normal(sp)*nDir;


        // Basic box face based bump mapping.
        float bumpShade = 1.;
        if(svGID!=2){
            bumpShade = bumpSurf3D(sp, sn);
            sn = doBumpMap(sp, sn, .5);
        }


        #if 1
        // Point lighting.

        // Light direction vector.
        vec3 ld = lightPos - sp;

        // Distance from the light to the surface point.
        float lDist = max(length(ld), 1e-5);

        // Normalize the light direction vector.
        ld /= lDist;

        // Light attenuation, based on the distances above.
        float atten = 1./(1. + lDist*lDist*.05); // + distlpsp*distlpsp*0.025
        #else
        // Direct lighting.
        vec3 ld = normalize(-vec3(.0, -1, -3));
        float atten = 1.;
        float lDist = FAR;
        #endif


        // Ambient occlusion and shadows.
        float ao = calcAO(sp, sn);
        float sh = softShadow(sp, ld, sn, lDist, 8.);




        ////////////////////

        // Scene object coloring.

        vec3 texCol;

        // Block ID.
        vec3 id3 = svVal.yzw;

        // Texturing.
        vec3 txP = sp;
        vec3 txN = sn;
        txP.xy *= rot2(PI/4.);
        //txN.xy *= rot2(PI/4.);
        vec3 tx = tex3D(iChannel0, txP/6. + hash31(id3 + .21)*0., txN);
        float gr = dot(tx, vec3(.299, .587, .114));


        float rnd = hash31(id3 + .1);

        // Range and saturation. Normally constants.
        float range = hash31(id3 + .011)*.25; // 0 to 1.
        float saturation = .7;//hash31(id3 + .031)*.5; // 0 to 1.
        texCol = .5 + .45*cos(TAU*rnd*range + vec3(0, 1.57, 3.14)*saturation - .5);

        // Lighten the window frames.
        //if(svGID==1) texCol = texCol*.5 + .7;

        if(svGID!=2){
            // Tiles and window frame coloring.
            //texCol = vec3(rnd*.7 + .3);
            texCol *= smoothstep(0., .5, tx);//*.8 + .4;
            texCol = mix(texCol*.6,
                         vec3(rnd*.6 + .2)*dot(texCol, vec3(.299, .587, .114)), .3);
        }
        else{
            // Window coloring.
            texCol = getCol(id3);
            texCol *= tx + .25;
        }


        // Bump shading.
        if(bumpShade>.05 && bumpShade<.95) texCol *= bumpShade;


        //////////////////////


        // Ambient light.
        //
        // Quick Lighting Tech - blackle
        // https://www.shadertoy.com/view/ttGfz1
        // Studio and outdoor.
        //float amb = pow(length(sin(sn*2.)*.5 + .5), 2.);
        float amb = length(sin(sn*2.)*.5 + .5)/sqrt(3.)*smoothstep(-1., 1., sn.y);


        // Backscatter.
        float bac = clamp(dot(sn, -normalize(vec3(ld.x, 0, ld.z))), 0., 1.);
        bac = (bac*.5 + .5);//*bou; // Apply the back scatter.

        // Material properties.
        float fresRef; // Reflectivity.
        float type;    // Dielectric or metallic.
        float rough;   // Roughness.

        // Material tweaking.
        if(svGID==2){

            // Glass blocks.
            fresRef = .8; // High Fresnel reflection.
            rough = min(gr*1., 1.); // Mostly smooth.
            // Mostly dielectric.
            type = .2;

        }
        else {

            // Tiles and frames.
            if(svGID==1) // Window frames: Mostly metallic.
                type = .8;
            else type = .2; //Mostly dielectric.

            fresRef = .5;
            rough = min(gr*4., 1.);
        }




        // Standard BRDF dot product calculations.
        vec3 h = normalize(ld - rd); // Half vector.
        float ndl = dot(sn, ld);
        float nr = clamp(dot(sn, -rd), 0., 1.);
        float nl = clamp(ndl, 0., 1.);
        float nh = clamp(dot(sn, h), 0., 1.);
        float vh = clamp(dot(-rd, h), 0., 1.);

        // Specular microfacet (Cook- Torrance) BRDF.
        //
        // F0 for dielectics in range [0., .16]
        // Default FO is (.16 * .5^2) = .04
        // Common Fresnel values, F(0), or F0 here.
        // Water: .02, Plastic: .05, Glass: .08, Diamond: .17
        // Copper: vec3(.95, .64, .54), Aluminium: vec3(.91, .92, .92),
        // Gold: vec3(1, .71, .29), Silver: vec3(.95, .93, .88),
        // Iron: vec3(.56, .57, .58).
        vec3 f0 = vec3(.16*(fresRef*fresRef));
        // For metals, the base color is used for F0.
        f0 = mix(f0, texCol, type);
        vec3 FS = f0 + (1. - f0)*pow(1. - vh, 5.); // Fresnel-Schlick reflected light term.

        // BRDF style specular and diffuse calculations. There is so little
        // extra work involved, but the lighting quality is much better.
        vec3 spec = getSpec(FS, nh, nr, nl, rough);
        vec3 diff = getDiff(FS, nl, rough, type);


        // Add this after calculating the material based lighting above.
        // Using pseudo science to apply a bit of faux back scatter. :)
        float bl = max(dot(-normalize(vec3(ld.x, 0, ld.z)), sn), 0.);
        texCol = texCol + texCol*sky*bl*2.;

        // Slight overhead backgound lighting.
        texCol += texCol*sky*(sn.y*.35 + .65);
        //texCol *= 1. + sn.yzx*.25; // Normal color shading.


        // Combining the above terms to produce the final color.
        sceneCol = texCol*(diff*sh + amb*(sh*.5 + .5) + vec3(8)*spec*sh);

        // Shading.
        sceneCol *= atten*ao;

        // Faux specular reflection -- Requires the "Forest" cube map.
        float speR = pow(max(dot(normalize(ld - rd), sn), 0.), 8.);
        vec3 rf = reflect(rd, sn); // Surface reflection.
        vec3 rTx = texture(iChannel1, rf).xyz; rTx *= rTx;
        float rF = svGID==2? 8. : 4.;
        sceneCol = sceneCol + sceneCol*speR*rTx*rF;



    }

    // Applying the window glow.
    sceneCol += (sceneCol + .003)*svGlow*1.5;

    // Applying distance fog.
    sceneCol = mix(sceneCol, sky, smoothstep(.2, 1., t/FAR));


    // Blueish peripheral coloring. Not absolutely necessary, but the amateur
    // designer in me thinks it countered the excess warm tones. :)
    uv = fragCoord/iResolution.xy - .5;
    sceneCol = mix(sceneCol, sceneCol.zyx, smoothstep(0., .3, length(uv) - .425));

    // Toning down the color. The line above needs commenting out though
    //sceneCol = mix(sceneCol, vec3(1)*dot(sceneCol, vec3(.299, .587, .114)), .35);


    // Clamp and present the pixel to the screen.
    fragColor = vec4(max(sceneCol, 0.), t);

}
