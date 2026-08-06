import type { ApplicationAdapter, AtsKind } from "./contracts.js";
import { parseProviderApplicationTarget } from "./ats-target.js";

interface AdapterPattern {
  kind: Exclude<AtsKind, "semantic">;
  hosts: RegExp[];
  paths?: RegExp[];
}

export const adapterPatterns: AdapterPattern[] = [
  {
    kind: "workday",
    hosts: [/\.myworkdayjobs\.com$/i, /\.wd\d+\.myworkdayjobs\.com$/i],
  },
  {
    kind: "ashby",
    hosts: [/^jobs\.ashbyhq\.com$/i],
  },
  {
    kind: "smartrecruiters",
    hosts: [/^jobs\.smartrecruiters\.com$/i, /\.smartrecruiters\.com$/i],
  },
];

export function detectAts(rawUrl: string): AtsKind {
  const providerTarget = parseProviderApplicationTarget(rawUrl);
  if (providerTarget) return providerTarget.provider;
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return "semantic";
  }
  const match = adapterPatterns.find(
    (pattern) =>
      pattern.hosts.some((host) => host.test(url.hostname)) &&
      (!pattern.paths || pattern.paths.some((path) => path.test(url.pathname))),
  );
  return match?.kind ?? "semantic";
}

export class AdapterRegistry {
  private readonly adapters = new Map<AtsKind, ApplicationAdapter>();

  register(adapter: ApplicationAdapter): void {
    if (this.adapters.has(adapter.kind)) {
      throw new Error(`Adapter already registered: ${adapter.kind}`);
    }
    this.adapters.set(adapter.kind, adapter);
  }

  resolve(rawUrl: string): ApplicationAdapter {
    const initialTarget = parseProviderApplicationTarget(rawUrl);
    const kind = initialTarget?.provider ?? detectAts(rawUrl);
    const adapter = this.adapters.get(kind) ?? this.adapters.get("semantic");
    if (!adapter) throw new Error(`No adapter registered for ${kind}`);
    return adapter;
  }
}
