import{c as i}from"./index-BXeYcIAo.js";/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const s=[["circle",{cx:"12",cy:"12",r:"10",key:"1mglay"}],["path",{d:"m9 12 2 2 4-4",key:"dzmm74"}]],_=i("circle-check",s);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const c=[["circle",{cx:"12",cy:"12",r:"10",key:"1mglay"}],["polyline",{points:"12 6 12 12 16.5 12",key:"1aq6pp"}]],u=i("clock-3",c),r=[["role_mismatch","Wrong role"],["location","Location"],["compensation","Compensation"],["seniority","Seniority"],["company","Company"],["sponsorship","Sponsorship"],["already_applied","Already applied"],["not_interested","Not interested"],["other","Something else"]],l=[["site_problem","Job site problem"],["wrong_information","Wrong information"],["duplicate_application","Possible duplicate"],["submission_status","Submission status"],["billing","Billing or allowance"],["other","Something else"]],p=[["interview","Interview"],["offer","Offer"],["rejected","Not selected"],["withdrawn","Withdrawn"]];function m(e,t){return a(e,o=>o.event_type==="match_feedback"&&o.job_id===t)}function y(e,t){return m(e,t)?.action==="pass"}function f(e,t){return a(e,o=>o.event_type==="application_outcome"&&o.application_id===t)}function b(e,t){return e.filter(o=>o.event_type==="application_issue"&&o.application_id===t).sort(n)}function h(e){return[...r,...l,...p].find(([o])=>o===e)?.[1]||e.replaceAll("_"," ")}function a(e,t){return e.filter(t).sort(n)[0]}function n(e,t){return t.created_at_ms!==e.created_at_ms?t.created_at_ms-e.created_at_ms:t.id.localeCompare(e.id)}export{_ as C,u as a,b,p as c,l as d,h as e,y as i,f as l,r as m};
