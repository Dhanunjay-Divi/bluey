const DEFAULT_DOWNLOAD_NAME = "bluey-application-evidence";

export function safeDownloadFileName(value: string, fallback = DEFAULT_DOWNLOAD_NAME): string {
  const leaf = value.replaceAll("\\", "/").split("/").pop()?.trim() || "";
  const safe = leaf
    .replace(/[\u0000-\u001f\u007f<>:"|?*]/g, "-")
    .replace(/\s+/g, " ")
    .replace(/^\.+/, "")
    .slice(0, 180)
    .trim();
  return safe || fallback;
}

export function saveDownloadedBlob(blob: Blob, fileName: string): void {
  const objectUrl = URL.createObjectURL(blob);
  let link: HTMLAnchorElement | undefined;
  try {
    link = document.createElement("a");
    link.href = objectUrl;
    link.download = safeDownloadFileName(fileName);
    link.rel = "noopener";
    document.body.appendChild(link);
    link.click();
  } finally {
    link?.remove();
    URL.revokeObjectURL(objectUrl);
  }
}
