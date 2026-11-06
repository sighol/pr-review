import type { ComponentProps, MouseEvent } from "react";
import { isExternalUrl, openExternal } from "../lib/external";

/**
 * A link that opens in the default browser, never inside the app. Web and mail links are handed
 * to the system; `#anchors` are left alone; any other link is blocked so it can't replace the app.
 */
export function ExternalLink({ href, children, ...rest }: ComponentProps<"a">) {
  const open = (e: MouseEvent) => {
    if (!href || href.startsWith("#")) return;
    e.preventDefault();
    if (isExternalUrl(href)) openExternal(href);
  };
  return (
    <a {...rest} href={href} target="_blank" rel="noopener noreferrer" onClick={open} onAuxClick={open}>
      {children}
    </a>
  );
}
