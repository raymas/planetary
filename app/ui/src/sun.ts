/**
 * Shader-based Sun: an fBm-noise "boiling" surface, an additive glow halo,
 * and a Fresnel rim. Technique from
 * https://sangillee.com/2024-06-29-create-realistic-sun-with-shaders/
 *
 * The noise samples the object-space position (stable on the sphere) rather
 * than the article's view-space vector, so the pattern does not swim when
 * the camera moves; only u_time animates it.
 */

import * as THREE from "three";

const VERTEX = /* glsl */ `
#include <common>
#include <logdepthbuf_pars_vertex>
varying vec3 vPosObj;
varying vec3 vNormalView;
varying vec3 vViewPos;
void main() {
  vPosObj = normalize(position);
  vNormalView = normalize(normalMatrix * normal);
  vViewPos = normalize(vec3(modelViewMatrix * vec4(position, 1.0)).xyz);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  #include <logdepthbuf_vertex>
}
`;

const NOISE = /* glsl */ `
uniform float u_time;

float random (in vec3 st) {
  return fract(sin(dot(st, vec3(12.9898, 78.233, 23.112))) * 12943.145);
}

float noise (in vec3 _pos) {
  vec3 i_pos = floor(_pos);
  vec3 f_pos = fract(_pos);

  float i_time = floor(u_time * 0.2);
  float f_time = fract(u_time * 0.2);

  float aa = random(i_pos + i_time);
  float ab = random(i_pos + i_time + vec3(1., 0., 0.));
  float ac = random(i_pos + i_time + vec3(0., 1., 0.));
  float ad = random(i_pos + i_time + vec3(1., 1., 0.));
  float ae = random(i_pos + i_time + vec3(0., 0., 1.));
  float af = random(i_pos + i_time + vec3(1., 0., 1.));
  float ag = random(i_pos + i_time + vec3(0., 1., 1.));
  float ah = random(i_pos + i_time + vec3(1., 1., 1.));

  float ba = random(i_pos + (i_time + 1.));
  float bb = random(i_pos + (i_time + 1.) + vec3(1., 0., 0.));
  float bc = random(i_pos + (i_time + 1.) + vec3(0., 1., 0.));
  float bd = random(i_pos + (i_time + 1.) + vec3(1., 1., 0.));
  float be = random(i_pos + (i_time + 1.) + vec3(0., 0., 1.));
  float bf = random(i_pos + (i_time + 1.) + vec3(1., 0., 1.));
  float bg = random(i_pos + (i_time + 1.) + vec3(0., 1., 1.));
  float bh = random(i_pos + (i_time + 1.) + vec3(1., 1., 1.));

  vec3 t = smoothstep(0., 1., f_pos);
  float t_time = smoothstep(0., 1., f_time);

  return mix(
    mix(
      mix(mix(aa, ab, t.x), mix(ac, ad, t.x), t.y),
      mix(mix(ae, af, t.x), mix(ag, ah, t.x), t.y),
      t.z),
    mix(
      mix(mix(ba, bb, t.x), mix(bc, bd, t.x), t.y),
      mix(mix(be, bf, t.x), mix(bg, bh, t.x), t.y),
      t.z),
    t_time);
}

#define NUM_OCTAVES 6
float fBm (in vec3 _pos, in float sz) {
  float v = 0.0;
  float a = 0.2;
  _pos *= sz;

  vec3 angle = vec3(-0.001 * u_time, 0.0001 * u_time, 0.0004 * u_time);
  mat3 rotx = mat3(1, 0, 0,
                   0, cos(angle.x), -sin(angle.x),
                   0, sin(angle.x), cos(angle.x));
  mat3 roty = mat3(cos(angle.y), 0, sin(angle.y),
                   0, 1, 0,
                   -sin(angle.y), 0, cos(angle.y));
  mat3 rotz = mat3(cos(angle.z), -sin(angle.z), 0,
                   sin(angle.z), cos(angle.z), 0,
                   0, 0, 1);

  for (int i = 0; i < NUM_OCTAVES; ++i) {
    v += a * noise(_pos);
    _pos = rotx * roty * rotz * _pos * 2.0;
    a *= 0.8;
  }
  return v;
}
`;

const SURFACE_FRAG = /* glsl */ `
#include <logdepthbuf_pars_fragment>
${NOISE}
varying vec3 vPosObj;
void main() {
  #include <logdepthbuf_fragment>
  vec3 st = vPosObj;

  vec3 q = vec3(0.);
  q.x = fBm( st, 5.);
  q.y = fBm( st + vec3(1.2, 3.2, 1.52), 5.);
  q.z = fBm( st + vec3(0.02, 0.12, 0.152), 5.);

  float n = fBm(st + q + vec3(1.82, 1.32, 1.09), 5.);

  vec3 color = vec3(0.);
  color = mix(vec3(1., 0.4, 0.), vec3(1., 1., 1.), n * n);
  color = mix(color, vec3(1., 0., 0.), q * 0.7);
  gl_FragColor = vec4(1.6 * color, 1.);
}
`;

const GLOW_FRAG = /* glsl */ `
#include <logdepthbuf_pars_fragment>
uniform vec3 u_color;
varying vec3 vViewPos;
varying vec3 vNormalView;
void main() {
  #include <logdepthbuf_fragment>
  float raw_intensity = max(dot(vViewPos, vNormalView), 0.);
  float intensity = pow(raw_intensity, 4.);
  gl_FragColor = vec4(u_color, intensity);
}
`;

const FRESNEL_FRAG = /* glsl */ `
#include <logdepthbuf_pars_fragment>
uniform vec3 u_color;
varying vec3 vViewPos;
varying vec3 vNormalView;
void main() {
  #include <logdepthbuf_fragment>
  float fresnelTerm_inner = 0.2 - 0.7 * min(dot(vViewPos, vNormalView), 0.0);
  fresnelTerm_inner = pow(fresnelTerm_inner, 5.0);

  float fresnelTerm_outer = 1.0 + dot(normalize(vViewPos), normalize(vNormalView));
  fresnelTerm_outer = pow(fresnelTerm_outer, 2.0);

  float fresnelTerm = fresnelTerm_inner + fresnelTerm_outer;
  gl_FragColor = vec4(u_color, 0.7) * fresnelTerm;
}
`;

export interface SunShaders {
  surface: THREE.ShaderMaterial;
  glow: THREE.ShaderMaterial;
  fresnel: THREE.ShaderMaterial;
}

/** The Sun's three materials: noise surface, glow halo (BackSide), rim. */
export function makeSunShaders(): SunShaders {
  const surface = new THREE.ShaderMaterial({
    vertexShader: VERTEX,
    fragmentShader: SURFACE_FRAG,
    uniforms: { u_time: { value: 0 } },
  });
  const glow = new THREE.ShaderMaterial({
    vertexShader: VERTEX,
    fragmentShader: GLOW_FRAG,
    uniforms: { u_color: { value: new THREE.Color(1.0, 0.5, 0.12) } },
    transparent: true,
    blending: THREE.AdditiveBlending,
    side: THREE.BackSide,
    depthWrite: false,
  });
  const fresnel = new THREE.ShaderMaterial({
    vertexShader: VERTEX,
    fragmentShader: FRESNEL_FRAG,
    uniforms: { u_color: { value: new THREE.Color(1.0, 0.55, 0.15) } },
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
  });
  return { surface, glow, fresnel };
}
