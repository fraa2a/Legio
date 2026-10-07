export function decodeImageResponse<T>(payload: ArrayBuffer): T & { bytes: Uint8Array<ArrayBuffer> } {
  if (payload.byteLength < 4) throw new Error("Artwork response is truncated");
  const length = new DataView(payload).getUint32(0, true);
  if (length > 4096 || length + 4 >= payload.byteLength) throw new Error("Artwork response has invalid metadata");
  const metadata = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(payload, 4, length))) as T;
  return { ...metadata, bytes: new Uint8Array(payload, 4 + length) };
}
