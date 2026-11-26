export type ResumeSelectionState<TFile, TParsed> =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; token: symbol; file: TFile; parsed: TParsed }
  | { kind: "error"; error: unknown };

export function createLatestResumeSelection<TFile, TParsed>(
  read: (file: TFile) => Promise<TParsed>,
) {
  let current = Symbol("resume-selection-invalid");

  function isCurrent(token: symbol): boolean {
    return current === token;
  }

  function invalidate(): void {
    current = Symbol("resume-selection-invalid");
  }

  async function select(
    file: TFile,
    publish: (state: ResumeSelectionState<TFile, TParsed>) => void,
  ): Promise<void> {
    const token = Symbol("resume-selection");
    current = token;
    publish({ kind: "loading" });

    let parsed: TParsed;
    try {
      parsed = await read(file);
    } catch (error) {
      if (isCurrent(token)) publish({ kind: "error", error });
      return;
    }
    if (isCurrent(token)) publish({ kind: "ready", token, file, parsed });
  }

  return { select, invalidate, isCurrent };
}
