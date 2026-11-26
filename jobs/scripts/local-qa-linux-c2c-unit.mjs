// Read-only admission proof for an administrator-prepared synthetic C2C unit.
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { constants } from "node:fs";
import { lstat, open, readlink, realpath } from "node:fs/promises";
import { resolve } from "node:path";
import { parseLocalQaBuildBinding } from "./local-qa-build-binding.mjs";
import { assertLocalQaLinuxPostgresRunRoot } from "./local-qa-linux-postgres.mjs";
import { parseLinuxUnitMainStat } from "./local-qa-linux-postmaster-observer.mjs";

const REFUSAL = "BLUEY_LOCAL_QA_LINUX_C2C_UNIT_REFUSED";
const FIELDS = [
  "Id", "InvocationID", "MainPID", "ControlGroup", "PrivateNetwork", "NoNewPrivileges", "CapabilityBoundingSet",
  "Restart", "KillMode", "ActiveState", "SubState", "Type", "NetworkNamespacePath", "JoinsNamespaceOf", "PrivateTmp",
  "AmbientCapabilities", "ProtectSystem", "ProtectHome", "ExitType", "ReadWritePaths", "RestrictAddressFamilies",
];
const need = value => { if (!value) throw new Error(REFUSAL); };
const hash = value => typeof value === "string" && value.length === 64 && /^[0-9a-f]+$/.test(value);
const pid = value => Number.isSafeInteger(value) && value > 1 && value <= 2_147_483_647;
const plain = value => value && Object.getPrototypeOf(value) === Object.prototype && !Array.isArray(value);
const stamp = s => [s.dev,s.ino,s.uid,s.mode,s.nlink,s.size,s.mtimeNs,s.ctimeNs].map(String).join(":");

export function parseLocalQaLinuxC2cUnitShow(text) {
  try {
    need(typeof text === "string" && text.length > 0 && text.length <= 16384 && text.endsWith("\n") && !/\0|\r/.test(text));
    const out = {};
    for (const line of text.slice(0, -1).split("\n")) { const i = line.indexOf("="); need(i > 0); const k = line.slice(0,i), v = line.slice(i+1); need(FIELDS.includes(k) && !Object.hasOwn(out,k) && !/[\n\r\0]/.test(v)); out[k]=v; }
    need(Object.keys(out).length === FIELDS.length && FIELDS.every(key => Object.hasOwn(out,key)));
    return Object.freeze(out);
  } catch { throw new Error(REFUSAL); }
}

export function validateLocalQaLinuxC2cUnitShow(value, expected) {
  try {
    need(plain(value) && plain(expected) && value.Id === expected.unitName && value.InvocationID === expected.invocationId
      && value.MainPID === String(expected.mainPid) && value.ControlGroup === expected.unitCgroup && value.PrivateNetwork === "yes"
      && value.NoNewPrivileges === "yes" && value.CapabilityBoundingSet === "" && value.Restart === "no"
      && value.KillMode === "control-group" && value.ActiveState === "active" && value.SubState === "running" && value.Type === "exec"
      && value.NetworkNamespacePath === "" && value.JoinsNamespaceOf === "" && value.PrivateTmp === "no" && value.AmbientCapabilities === ""
      && value.ProtectSystem === "strict" && value.ProtectHome === "yes" && value.ExitType === "cgroup"
      && value.ReadWritePaths === expected.launcherParent
      && typeof value.RestrictAddressFamilies === "string"
      && value.RestrictAddressFamilies.split(" ").sort().join(",") === "AF_INET,AF_INET6,AF_NETLINK,AF_UNIX");
    return true;
  } catch { throw new Error(REFUSAL); }
}

async function pin(path, expected) {
  need(typeof path === "string" && resolve(path) === path && hash(expected) && await realpath(path) === path);
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try { const before = await file.stat({ bigint:true }); need(before.isFile() && before.uid === 0n && before.size > 0n && before.size <= 256n*1024n*1024n && (before.mode&0o7022n)===0n && (before.mode&0o111n)!==0n);
    const h=createHash("sha256"), b=Buffer.allocUnsafe(65536); let n=0; for(;;){const {bytesRead}=await file.read(b,0,b.length,null);if(!bytesRead)break;n+=bytesRead;need(n<=256*1024*1024);h.update(b.subarray(0,bytesRead));}
    const after=await file.stat({bigint:true}), named=await lstat(path,{bigint:true}); need(BigInt(n)===before.size&&stamp(before)===stamp(after)&&stamp(before)===stamp(named)&&h.digest("hex")===expected); return stamp(before);
  } finally { await file.close(); }
}

async function show(systemctl, unitName) {
  return await new Promise((resolve, reject) => {
    const child = spawn(systemctl, ["--system", "--no-pager", "show", "--all", ...FIELDS.map(name => `--property=${name}`),
      "--", unitName], { env: { PATH: "/usr/bin:/bin", LC_ALL: "C" }, stdio: ["ignore", "pipe", "pipe"], detached: false });
    let output = "", size = 0, failed = false, closed = false, reap;
    const stop = () => {
      failed = true;
      // This handle is only the read-only systemctl child we just spawned.
      if (!closed && !child.killed) child.kill("SIGKILL");
      reap ??= setTimeout(() => reject(new Error(REFUSAL)), 1_000);
    };
    const deadline = setTimeout(stop, 5_000);
    child.stdout.on("data", chunk => {
      size += chunk.length;
      if (size > 16_384) stop();
      else output += chunk.toString("utf8");
    });
    child.stderr.on("data", stop);
    child.stdout.on("error", stop); child.stderr.on("error", stop);
    child.once("error", stop);
    child.once("close", (code, signal) => {
      closed = true;
      clearTimeout(deadline); clearTimeout(reap);
      if (!failed && code === 0 && signal === null) resolve(output);
      else reject(new Error(REFUSAL));
    });
  });
}

async function text(path, max = 4096) {
  const file = await open(path, constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW);
  try {
    const buffer = Buffer.alloc(max + 1);
    let size = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, size, buffer.length - size, null);
      size += bytesRead;
      need(size <= max);
      if (bytesRead === 0) break;
    }
    const bytes = buffer.subarray(0, size);
    need(bytes.length > 0 && !bytes.includes(0));
    return bytes.toString("utf8");
  } finally { await file.close(); }
}

export async function createLocalQaLinuxC2cUnitAssertion({root,unitName,invocationId,bootId,mainPid,unitCgroup,networkNamespace,systemctl,buildBinding}) {
  try {
    need(process.platform === "linux" && process.getuid?.() > 0 && typeof unitName === "string" && /^bluey-jobs-qa-[a-z0-9_.@-]+\.service$/.test(unitName)
      && typeof invocationId === "string" && /^[0-9a-f]{32}$/.test(invocationId) && typeof bootId === "string" && /^[0-9a-f]{32}$/.test(bootId)
      && pid(mainPid) && unitCgroup === `/system.slice/${unitName}`
      && typeof networkNamespace === "string" && /^net:\[[1-9][0-9]{0,19}\]$/.exec(networkNamespace)?.[0] === networkNamespace
      && plain(systemctl) && Object.keys(systemctl).sort().join(",") === "path,sha256" && systemctl.path === "/usr/bin/systemctl");
    const binding=parseLocalQaBuildBinding(buildBinding), rootStamp=await assertLocalQaLinuxPostgresRunRoot(root), systemctlStamp=await pin(systemctl.path,systemctl.sha256);
    const expected = Object.freeze({ unitName, invocationId, mainPid, unitCgroup, launcherParent: root.slice(0, root.lastIndexOf("/")) });
    const tool = Object.freeze({ path: systemctl.path, sha256: systemctl.sha256 });
    const sample = async () => {
      need(await assertLocalQaLinuxPostgresRunRoot(root) === rootStamp);
      const boot = await text("/proc/sys/kernel/random/boot_id", 128);
      need(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\n$/.test(boot)
        && boot.trim().replaceAll("-", "") === bootId);
      for (const proc of ["/proc/self", `/proc/${mainPid}`]) {
        need(await text(`${proc}/cgroup`) === `0::${unitCgroup}\n` && await readlink(`${proc}/ns/net`) === networkNamespace);
      }
      return parseLinuxUnitMainStat(await text(`/proc/${mainPid}/stat`), mainPid);
    };
    const initial = await sample();
    const matches = next => next.pid === initial.pid && next.pgrp === initial.pgrp
      && next.ppid === initial.ppid && next.startTime === initial.startTime;
    return async observed => {
      try {
        need(plain(observed) && observed.root === root && observed.stateDirectory === `${root}/bluey-jobs-qa-draft`
          && observed.unitCgroup === unitCgroup && observed.networkNamespace === networkNamespace);
        const observedBuild = parseLocalQaBuildBinding(observed.buildBinding);
        need(Object.keys(binding).every(key => binding[key] === observedBuild[key]));
        need(matches(await sample()) && await pin(tool.path, tool.sha256) === systemctlStamp);
        validateLocalQaLinuxC2cUnitShow(parseLocalQaLinuxC2cUnitShow(await show(tool.path, unitName)), expected);
        need(await pin(tool.path, tool.sha256) === systemctlStamp && matches(await sample()));
        return true;
      } catch { throw new Error(REFUSAL); }
    };
  } catch { throw new Error(REFUSAL); }
}
