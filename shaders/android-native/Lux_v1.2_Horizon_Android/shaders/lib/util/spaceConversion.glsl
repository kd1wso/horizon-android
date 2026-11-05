/* 
----------------------------------------------------------------
Lux Shader by https://github.com/TechDevOnGithub/
Based on BSL Shaders v7.1.05 by Capt Tatsu https://bitslablab.com 
See AGREEMENT.txt for more information.
----------------------------------------------------------------
*/ 

#define diagonal3(m) vec3((m)[0].x, (m)[1].y, m[2].z)
#define projMAD(m, v) (diagonal3(m) * (v) + (m)[3].xyz)

// Horizon: the whole inverse, not only its perspective terms. Horizon's nightly of 2026-10-31
// keeps the view bob inside gbufferProjection, so the diagonal shortcut moved every rebuilt
// position (and the shadows with them) as the camera bobbed.
vec3 ToNDC(vec3 pos)
{
    vec4 viewPos = gbufferProjectionInverse * vec4(pos * 2.0 - 1.0, 1.0);
    return viewPos.xyz / viewPos.w;
}

vec3 ToWorld(vec3 pos)
{
	return mat3(gbufferModelViewInverse) * pos + gbufferModelViewInverse[3].xyz;
}

vec3 ToShadow(vec3 pos, mat4 viewMat, mat4 projMat)
{
	vec3 shadowpos = mat3(viewMat) * pos + viewMat[3].xyz;
	return projMAD(projMat, shadowpos);
}