// Original mark: a weathervane arrow whose tail feathers become three sound-wave bars.
export function LogoGlyph({ size = 18, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true" className={className}>
      <g stroke="currentColor" strokeLinecap="round" strokeWidth="3.2">
        <path d="M4.5 13v6" />
        <path d="M9.5 9v14" />
        <path d="M14.5 12v8" />
        <path d="M14.5 16h8.5" />
      </g>
      <path d="M21.6 10.4a1.2 1.2 0 0 1 1.8-1l6.7 5.6a1.3 1.3 0 0 1 0 2l-6.7 5.6a1.2 1.2 0 0 1-1.8-1Z" fill="currentColor" />
    </svg>
  );
}

export function LogoMark({ size = 28 }: { size?: number }) {
  return (
    <span className="logo-mark" style={{ width: size, height: size }}>
      <LogoGlyph size={Math.round(size * 0.66)} />
    </span>
  );
}
