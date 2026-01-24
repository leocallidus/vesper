
// Standard 2D rotation formula.
mat2 rot2(in float a){ float c = cos(a), s = sin(a); return mat2(c, -s, s, c); }

// A slight variation on one of Dave Hoskins's hash functions,
// which you can find here:
//
// Hash without Sine -- Dave Hoskins
// https://www.shadertoy.com/view/4djSRW
// 1 out, 3 in...
float hash31(vec3 p3)
{
    p3  = fract(p3*vec3(.6031, .5030, .4973));
    p3 += dot(p3, p3.zyx + 43.527);
    return fract((p3.x + p3.y) * p3.z);
}

// Dave's hash function. More reliable with large values, but will still eventually
// break down.
//
// Hash without Sine.
// Creative Commons Attribution-ShareAlike 4.0 International Public License.
// Created by David Hoskins.
// vec3 to vec3.
vec3 hash33(vec3 p){

    p = fract(p * vec3(.10313, .10307, .09731));
    p += dot(p, p.yxz + 19.1937);
    return fract((p.xxy + p.yxx)*p.zyx);

}

////////////


// Bidirectional Reflectance Distribution Function (BRDF).
//
// If you want a quick crash course in BRDF, see the following:
// Microfacet BRDF: Theory and Implementation of Basic PBR Materials
// https://www.youtube.com/watch?v=gya7x9H3mV0&t=730s
//

// Surface geometry function.
float GGX_Schlick(float nv, float rough) {
    //float r = roughness; // original
    float r = .5 + .5*rough; // Disney remapping.
    float k = (r*r)/2.;
    float denom = nv*(1. - k) + k;
    return max(nv, .001)/denom;
}

float G_Smith(float nr, float nl, float rough) {
    float g1_l = GGX_Schlick(nl, rough);
    float g1_v = GGX_Schlick(nr, rough);
    return g1_l*g1_v;
}

// Specular calculation.
vec3 getSpec(vec3 FS, float nh, float nr, float nl, float rough){

    // Microfacet distribution... Most dominant term.
    // Microfaceted normal distribution function.
    float alpha = pow(rough, 4.);
    float b = (nh*nh*(alpha - 1.) + 1.);
    float D = alpha/(3.14159265*b*b);

    // Geometry self shadowing term.
    float G = G_Smith(nr, nl, rough);

    // Combining the terms above.
    return FS*D*G/(4.*max(nr, .001))*3.14159265;
}

vec3 getDiff(vec3 FS, float nl, float rough, float type){

    // Diffuse calculations.
    vec3 diff = nl*(1. - FS); // If not specular, use as diffuse (optional)
    return diff*(1. - type); // No diffuse for metals.
}


///////////
// Cube mapping - Adapted from one of Fizzer's routines.  This one is
// technically cuboid mapping.
vec3 cubeMap(vec3 p, vec3 gSc){

    // Scaling the coordinates.
    vec3 svP = p;
    p /= gSc;

    // Elegant cubic space stepping trick, as seen in many voxel related examples.
    vec3 f = abs(p); f = step(f.zxy, f)*step(f.yzx, f);

    // Cube face number.
    ivec3 idF = ivec3(p.x<.0? 0 : 1, p.y<.0? 2 : 3, p.z<0.? 4 : 5);

    // Local face coordinates and face ID.
    return f.x>.5? vec3(svP.zy, idF.x) :
    f.y>.5? vec3(svP.xz, idF.y) : vec3(svP.xy, idF.z);

}
