/**
 * The API's error envelope, as a thrown object.
 *
 * `docs/API.md` promises a stable `code`, a human message and optional details that never contain
 * a token or a path. That promise is what lets the UI show `error.message` verbatim.
 */

export const ERROR_HINTS: Record<string, string> = {
  invalid_query: 'The request was malformed.',
  invalid_body: 'The request body was malformed.',
  not_found: 'Nothing to show yet.',
  unknown_pack: 'That pack is not in the ledger.',
  stale_write: 'A newer value exists; the write would rewrite history.',
  validation_failed: 'The API refused that value.',
  invalid_setting: 'That setting value is out of range.',
  unsupported_setting: 'The API does not expose that setting.',
  engine_unavailable: 'No engine loop is running, so nothing was queued.',
  corrupt_plan: 'A stored plan summary cannot be rendered.',
  internal: 'The server hit an unexpected error.',
};

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: Record<string, unknown> | null;

  constructor(status: number, code: string, message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.details = details ?? null;
  }

  /** Human-readable next step for the codes the UI can actually recover from. */
  get hint(): string {
    return ERROR_HINTS[this.code] ?? ERROR_HINTS.internal;
  }

  /** Envelope parser; never throws, so a non-JSON body still becomes an `ApiError`. */
  static async fromResponse(response: Response): Promise<ApiError> {
    let code = response.status === 404 ? 'not_found' : 'internal';
    let message = `HTTP ${response.status}`;
    let details: Record<string, unknown> | undefined;
    try {
      const body: unknown = await response.json();
      if (body !== null && typeof body === 'object' && 'error' in body) {
        const envelope = (body as { error?: unknown }).error;
        if (envelope !== null && typeof envelope === 'object') {
          const fields = envelope as Record<string, unknown>;
          if (typeof fields.code === 'string') code = fields.code;
          if (typeof fields.message === 'string') message = fields.message;
          if (fields.details !== null && typeof fields.details === 'object') {
            details = fields.details as Record<string, unknown>;
          }
        }
      }
    } catch {
      // A body that is not JSON still has a status; the default message is the honest answer.
    }
    return new ApiError(response.status, code, message, details);
  }
}

export function isApiError(value: unknown): value is ApiError {
  return value instanceof ApiError;
}

/** Turn anything thrown into the one-line sentence a toast can show. */
export function describeError(value: unknown): string {
  if (isApiError(value)) return `${value.message} (${value.code})`;
  if (value instanceof Error) return value.message;
  return String(value);
}
