import { openModExternalLink } from "../../services/modding";
import { showToast } from "../../stores/toast";
import { toMessage } from "../../utils/errors";

// Opens an address built by game-links.ts in the system browser.
export async function openGameLink(url: string): Promise<void> {
  try {
    await openModExternalLink(url);
  } catch (cause) {
    showToast(toMessage(cause), "error");
  }
}
