import{c as i}from"./index-Cmi0aWMP.js";/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const c=[["rect",{width:"20",height:"16",x:"2",y:"4",rx:"2",key:"18n3k1"}],["path",{d:"m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7",key:"1ocrg3"}]],t=i("mail",c);function e(o){return/^company-v1-[a-f0-9]{64}$/u.test(o??"")}function a(o,r,n){if(!o||o.job_id!==r||!r||!e(o.scope_id)||typeof o.company!="string"||!o.company.trim()||typeof o.domain!="string"||!o.domain.trim())throw new Error("This employer could not be verified. Choose a current job and try again.");if(n?.scope==="company"&&e(n.scope_id)&&n.scope_id!==o.scope_id)throw new Error("This saved employer reference does not match. Add a new saved answer instead.");return o}function p(o){return!!(o.interventionId&&o.binding?.job_id===o.jobId&&o.loadedInterventionId===o.interventionId&&o.confirmed&&!o.loading&&!o.error)}export{t as M,p as c,e as i,a as v};
