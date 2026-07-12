import{c as s}from"./index-8Nuom8wF.js";/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const l=[["circle",{cx:"12",cy:"12",r:"10",key:"1mglay"}],["line",{x1:"10",x2:"10",y1:"15",y2:"9",key:"c1nkhi"}],["line",{x1:"14",x2:"14",y1:"15",y2:"9",key:"h65svq"}]],p=s("circle-pause",l);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const c=[["path",{d:"M15 3h6v6",key:"1q9fwt"}],["path",{d:"M10 14 21 3",key:"gplh6r"}],["path",{d:"M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",key:"a6xqqp"}]],d=s("external-link",c);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const n=[["path",{d:"m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",key:"wmoenq"}],["path",{d:"M12 9v4",key:"juzpu7"}],["path",{d:"M12 17h.01",key:"p32p05"}]],y=s("triangle-alert",n);function h(e){return e.filter(t=>t.state==="queued")}function k(e,t){return t==="auto_submit"&&e.eligibility?.can_auto_submit===!0?"auto_submit":"review_first"}function f(e){if(!e||e.status!=="open"||e.kind!=="browser_takeover"||e.resolution_kind!=="browser_takeover"||e.resume_after_resolution!==!0||e.choices.length!==0)return!1;const t=i(e.metadata.receipt),r=i(t.intervention),a=i(r.resolution),o=e.title==="Review the Greenhouse application"&&e.detail==="Review every employer-facing field and document in the preserved form, then approve submission."||e.title==="Review this Lever application"&&e.detail==="Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review.";return t.status==="needs_input"&&Array.isArray(t.issues)&&t.issues.length===0&&r.kind==="browser_takeover"&&typeof r.takeoverUrl=="string"&&r.takeoverUrl.length>0&&r.title===e.title&&r.detail===e.detail&&a.kind==="browser_takeover"&&a.resumeAfter===!0&&o}function i(e){return e&&typeof e=="object"&&!Array.isArray(e)?e:{}}export{p as C,d as E,y as T,k as e,f as i,h as r};
