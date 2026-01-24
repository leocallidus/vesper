/*
 *
 *    Warped Offset Block Corridor
 *    ----------------------------
 *
 *    See "Buffer A" for an explanation.
 *
 */


/*
 * // Just a very basic depth of field routine -- I find a lot of it is
 * // common sense. Basically, you store the scene distance from the camera
 * // in the fourth channel, then use it to determine how blurry you want
 * // your image to be at that particular distance.
 * //
 * // For instance, in this case, I want pixels that are 2.25 units away from
 * // the camera to be in focus (not blurred) and for things to get more
 * // blurry as you move away from that point -- aptly named the focal point
 * // for non camera people. :)
 * //
 * // I based this on old code of mine, but adopted things that I found in
 * // IQ and Nesvi7's examples, which you can find here:
 * //
 * // Ladybug - IQ
 * // https://www.shadertoy.com/view/4tByz3
 * //
 * // Cube surface II - Nesvi7
 * // https://www.shadertoy.com/view/Mty3DV
 * //
 * vec3 DpthFld(sampler2D iCh, vec2 uv){
 *
 *    #if 0
 *    // Focal point and circle of confusion.
 *    const float focD = 8., coc = 1.;
 *    // Linear distance from either side of the focal point.
 *    float l = abs(texture(iCh, uv).w - focD) - coc;
 *    // Using it to calculate the DOF.
 *    float dof = clamp(l/coc, 0., 2.)*2.;
 *    #else
 *    // Faux peripheral DOF.
 *    vec2 uv2 = (uv - .5)*vec2(iResolution.y/iResolution.x, 1);
 *    float dof = smoothstep(0., .35, length(uv2) - .3)*1.5;
 *    #endif
 *
 *    // Combine samples. Samples with a larger DOF value are taken further
 *    // away from the original point, and as such appear blurrier.
 *    vec3 acc = vec3(0);
 *
 *    for(int i = 0; i<25; i++){
 *        // Accumulate samples.
 *        acc += texture(iCh, uv + (vec2(i/5, i%5) - 2.)/iResolution.y*dof).xyz;
 *        //acc.x *= dof/2.;
 *    }
 *
 *    // Return the new variably blurred value.
 *    return acc /= 25.;
 *    // Visual debug representation of DOF value.
 *    //return vec3(length(dof)*450./2.5);
 * }
 */


void mainImage(out vec4 fragColor, in vec2 fragCoord){


    // Screen oordinates.
    vec2 uv = fragCoord/iResolution.xy;

    // The DOF looks nice, but I'm going for a cleaner, and cheaper, look.
    //vec4 col = DpthFld(iChannel0, uv).xyzz;

    // Retrieving the stored color.
    vec4 col = texture(iChannel0, uv);



    // Hardware bloom that I made up on the spot. It's
    // not as nice as software bloom, but it's way cheaper
    // and definitely easier to implement.
    float a = 1., w = 1.;
    vec4 col2 = vec4(0);
    for (int i = 0; i<6; i++){
        vec2 jit = (texture(iChannel1, uv + float(i)/6. +
        fract(iTime)).xy - .5)/iResolution.y;
        col2 += texture(iChannel0, uv + jit*16., float(i)/2.)*w;
        a += w;
        w *= .7071;
    }
    col2 /= a;

    col += smoothstep(vec4(.05), vec4(1), col2);


    // Subtle Reinhard-based tone-mapping.
    //col /= (2. + col)/2.25;

    // Basic high frequency sigmoid tone mapping, popularized by XOR.
    col = tanh(col);


    // Subtle vignette.
    col *= pow(16.*uv.x*uv.y*(1. - uv.x)*(1. - uv.y) , 1./16.);

    // Rough gamma correction and screen presentation.
    fragColor = pow(max(col, 0.), vec4(1./2.2));

}
