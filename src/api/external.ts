import { commands } from "../bindings";

/**
 * Send a link to the system browser.
 *
 * The WebView must never navigate to a URL that arrived in a message: the app
 * is the page, and following the link would replace the interface with whatever
 * the other device sent. The backend also refuses anything but http, https and
 * mailto, so a scheme check here would only duplicate it.
 *
 * A failure is reported rather than swallowed: a link that does nothing looks
 * like a broken message.
 */
export async function openExternal(url: string): Promise<void> {
  const result = await commands.openExternal(url);
  if (result.status === "error") {
    throw new Error(result.error.message);
  }
}
