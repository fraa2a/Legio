import { invoke as nativeInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";
import { applicationErrorStack, describeApplicationError, logApplicationEvent } from "./application-log";

export async function invoke<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  try {
    return await nativeInvoke<T>(command, args, options);
  } catch (error) {
    if (!(error !== null && typeof error === "object" && "kind" in error && error.kind === "cancelled")) logApplicationEvent("error", "command_error", `${command}: ${describeApplicationError(error)}`, applicationErrorStack(error));
    throw error;
  }
}
