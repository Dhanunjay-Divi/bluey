export interface SearchRequestToken {
  generation: number;
  query: string;
}

export function shouldApplySearchResponse(
  activeGeneration: number,
  activeQuery: string,
  token: SearchRequestToken,
  aborted: boolean,
): boolean {
  return (
    !aborted &&
    activeGeneration === token.generation &&
    activeQuery === token.query
  );
}
