export function Placeholder({ name }: { name: string }) {
  return (
    <div className="flex h-full items-center justify-center">
      <h2 className="text-2xl font-semibold text-zinc-500">{name}</h2>
    </div>
  );
}
