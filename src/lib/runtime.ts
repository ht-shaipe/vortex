export interface Runtime {
  kind: 'desktop' | 'web'
}

function detectKind(): Runtime['kind'] {
  return '__TAURI_INTERNALS__' in window ? 'desktop' : 'web'
}

export const runtime: Runtime = { kind: detectKind() }