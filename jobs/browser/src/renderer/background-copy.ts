export interface BackgroundAvailabilityCopyInput {
  backgroundEnabled: boolean;
  loginItemSupported: boolean;
}

export function backgroundAvailabilityDetail(
  input: BackgroundAvailabilityCopyInput,
): string {
  if (input.backgroundEnabled) {
    return input.loginItemSupported
      ? "Starts quietly at sign-in and stays ready in the tray while this computer is awake."
      : "Stays ready in the tray while this computer is awake. Start it again after signing in.";
  }
  return input.loginItemSupported
    ? "Turn on to start quietly at sign-in and stay ready in the tray while this computer is awake."
    : "Turn on to stay ready in the tray while this computer is awake.";
}

export function backgroundAvailabilityTitle(
  input: BackgroundAvailabilityCopyInput,
): string {
  if (input.backgroundEnabled) return "Ready in the background";
  return input.loginItemSupported
    ? "Start at sign-in"
    : "Keep ready in the background";
}

export function backgroundAvailabilityStatus(
  input: Pick<BackgroundAvailabilityCopyInput, "backgroundEnabled">,
): string {
  return input.backgroundEnabled ? "Background ready" : "Window only";
}
