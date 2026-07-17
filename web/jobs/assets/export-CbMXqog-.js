const __vite__mapDeps=(i,m=__vite__mapDeps,d=(m.f||(m.f=["assets/jspdf.es.min-Bckz6f0s.js","assets/index-DiTCwTOq.js","assets/index-D6l0iYOp.css"])))=>i.map(i=>d[i]);
import{c as d,_ as m}from"./index-DiTCwTOq.js";/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const w=[["path",{d:"M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4",key:"ih7n3h"}],["polyline",{points:"7 10 12 15 17 10",key:"2ggqvy"}],["line",{x1:"12",x2:"12",y1:"15",y2:"3",key:"1vk2je"}]],g=d("download",w);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const y=[["path",{d:"M4 22h14a2 2 0 0 0 2-2V7l-5-5H6a2 2 0 0 0-2 2v4",key:"1pf5j1"}],["path",{d:"M14 2v4a2 2 0 0 0 2 2h4",key:"tnqrlb"}],["path",{d:"m3 15 2 2 4-4",key:"1lhrkk"}]],D=d("file-check-2",y);/**
 * @license lucide-react v0.500.0 - ISC
 *
 * This source code is licensed under the ISC license.
 * See the LICENSE file in the root directory of this source tree.
 */const f=[["path",{d:"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",key:"1rqfz7"}],["path",{d:"M9 10h6",key:"9gxzsh"}],["path",{d:"M12 13V7",key:"h0r20n"}],["path",{d:"M9 17h6",key:"r8uit2"}]],F=d("file-diff",f);async function P(e,c){const{Document:n,HeadingLevel:o,Packer:a,Paragraph:t,TextRun:l}=await m(async()=>{const{Document:i,HeadingLevel:r,Packer:p,Paragraph:_,TextRun:k}=await import("./index-DI_3s6Kx.js");return{Document:i,HeadingLevel:r,Packer:p,Paragraph:_,TextRun:k}},[]),s=[new t({heading:o.TITLE,children:[new l({text:e.contact?.name||"Resume",bold:!0})]}),new t([e.contact?.email,e.contact?.phone,e.contact?.location].filter(Boolean).join(" | ")),new t({heading:o.HEADING_1,text:e.headline||"Professional Summary"}),new t(e.summary||""),new t({heading:o.HEADING_1,text:"Skills"}),new t((e.skills||[]).join(" | "))];for(const i of e.employment||[])s.push(new t({heading:o.HEADING_2,text:`${i.title} - ${i.company}`}),new t(`${i.start_date} - ${i.current?"Present":i.end_date}`),...i.highlights.map(r=>new t({text:r,bullet:{level:0}})));const u=await a.toBlob(new n({sections:[{children:s}]}));x(u,`${c}.docx`)}async function R(e,c){const{jsPDF:n}=await m(async()=>{const{jsPDF:l}=await import("./jspdf.es.min-Bckz6f0s.js").then(s=>s.j);return{jsPDF:l}},__vite__mapDeps([0,1,2])),o=new n({unit:"pt",format:"letter"}),a=54;let t=58;o.setFont("helvetica","bold"),o.setFontSize(20),o.text(e.contact?.name||"Resume",a,t),t+=22,o.setFont("helvetica","normal"),o.setFontSize(9),o.text([e.contact?.email,e.contact?.phone,e.contact?.location].filter(Boolean).join(" | "),a,t),t+=30,t=h(o,"SUMMARY",e.summary||"",a,t),t=h(o,"SKILLS",(e.skills||[]).join(" | "),a,t);for(const l of e.employment||[])t=h(o,`${l.title} - ${l.company}`,l.highlights.join(`
`),a,t);o.save(`${c}.pdf`)}function h(e,c,n,o,a){a>700&&(e.addPage(),a=58),e.setFont("helvetica","bold"),e.setFontSize(10),e.text(c,o,a),a+=16,e.setFont("helvetica","normal"),e.setFontSize(9);const t=e.splitTextToSize(n,500);return e.text(t,o,a),a+t.length*12+20}function x(e,c){const n=URL.createObjectURL(e),o=document.createElement("a");o.href=n,o.download=c,o.click(),URL.revokeObjectURL(n)}export{g as D,D as F,F as a,P as b,R as e};
