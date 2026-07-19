export interface BackgroundAvailabilityCopyInput {
  backgroundEnabled: boolean;
  loginItemSupported: boolean;
}

export function backgroundAvailabilityDetail(
  input: BackgroundAvailabilityCopyInput,
): string {
  if (input.backgroundEnabled) {
    return input.loginItemSupported
      ? "Closing this window hides it to the tray. It starts quietly at sign-in and runs only while this computer is awake."
      : "Closing this window hides it to the tray. Start-at-sign-in is not managed on this system.";
  }
  return input.loginItemSupported
    ? "Closing this window quits Bluey Browser. Turn this on to start quietly at sign-in and keep it available while this computer is awake."
    : "Closing this window quits Bluey Browser. Turn this on to keep it available while this computer is awake.";
}
