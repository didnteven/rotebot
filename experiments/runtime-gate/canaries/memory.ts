const allocations: Uint8Array[] = [];
while (true) {
  const bytes = new Uint8Array(16 * 1024 * 1024);
  bytes.fill(0xa5); // touch pages; ArrayBuffers bypass the V8 old-space flag
  allocations.push(bytes);
}
