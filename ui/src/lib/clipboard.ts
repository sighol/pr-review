import { describeError, log } from "./log";

/** Copies text to the clipboard; resolves to whether it worked. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (e) {
    log("warn", `navigator.clipboard failed, trying the fallback: ${describeError(e)}`);
  }
  // Older webviews only allow copying from a selected element.
  const area = document.createElement("textarea");
  area.value = text;
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  try {
    return document.execCommand("copy");
  } catch (e) {
    log("error", `could not copy to the clipboard: ${describeError(e)}`);
    return false;
  } finally {
    area.remove();
  }
}
