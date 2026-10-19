// Anthropic's Claude starburst, drawn as twelve rounded rays in the current text color.
const RAYS = Array.from({ length: 12 }, (_, index) => {
  const angle = (index * Math.PI) / 6;
  const length = index % 2 === 0 ? 9.5 : 7;
  const inner = 2.2;
  return {
    x1: (12 + inner * Math.cos(angle)).toFixed(2),
    y1: (12 + inner * Math.sin(angle)).toFixed(2),
    x2: (12 + length * Math.cos(angle)).toFixed(2),
    y2: (12 + length * Math.sin(angle)).toFixed(2),
  };
});

export function ClaudeIcon({ className }: { className?: string }) {
  return (
    <svg
      role="img"
      aria-label="Claude"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      className={className}
    >
      {RAYS.map((ray) => (
        <line key={`${ray.x2}-${ray.y2}`} x1={ray.x1} y1={ray.y1} x2={ray.x2} y2={ray.y2} />
      ))}
    </svg>
  );
}
