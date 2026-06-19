export function Placeholder({ name }: { name: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <h2 className="text-title-1 text-text-tertiary">{name}</h2>
    </div>
  );
}
