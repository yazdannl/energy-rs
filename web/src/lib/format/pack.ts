/**
 * What a pack is called on screen.
 *
 * The operator's alias wins, then the name discovery registered, then the serial. The serial is
 * always shown beside it, because it is the identity the API and the hardware agree on.
 */
export function packLabel(pack: {
  sn: string;
  alias?: string | null;
  name?: string | null;
}): string {
  return pack.alias ?? pack.name ?? pack.sn;
}
