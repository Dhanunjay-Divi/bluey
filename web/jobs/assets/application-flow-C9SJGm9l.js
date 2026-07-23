import{c as a}from"./index-DAGYUBpl.js";/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const c=[["circle",{cx:"12",cy:"12",r:"10",key:"1mglay"}],["line",{x1:"10",x2:"10",y1:"15",y2:"9",key:"c1nkhi"}],["line",{x1:"14",x2:"14",y1:"15",y2:"9",key:"h65svq"}]],n=a("circle-pause",c);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const l=[["path",{d:"M15 3h6v6",key:"1q9fwt"}],["path",{d:"M10 14 21 3",key:"gplh6r"}],["path",{d:"M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",key:"a6xqqp"}]],p=a("external-link",l);function d(e){return e.filter(t=>t.state==="queued")}function y(e,t){return t==="auto_submit"&&e.eligibility?.can_auto_submit===!0?"auto_submit":"review_first"}function k(e){if(!e||e.status!=="open"||e.kind!=="browser_takeover"||e.resolution_kind!=="browser_takeover"||e.resume_after_resolution!==!0||e.choices.length!==0)return!1;const t=i(e.metadata.receipt),r=i(t.intervention),s=i(r.resolution),o=e.title==="Review the Greenhouse application"&&e.detail==="Review every employer-facing field and document in the preserved form, then approve submission."||e.title==="Review this Lever application"&&e.detail==="Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review.";return t.status==="needs_input"&&Array.isArray(t.issues)&&t.issues.length===0&&r.kind==="browser_takeover"&&typeof r.takeoverUrl=="string"&&r.takeoverUrl.length>0&&r.title===e.title&&r.detail===e.detail&&s.kind==="browser_takeover"&&s.resumeAfter===!0&&o}function i(e){return e&&typeof e=="object"&&!Array.isArray(e)?e:{}}export{n as C,p as E,y as e,k as i,d as r};
