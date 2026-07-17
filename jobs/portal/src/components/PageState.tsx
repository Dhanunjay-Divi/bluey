import { AlertCircle, LoaderCircle } from "lucide-react";

export function LoadingScreen() {
  return <main className="center-screen"><LoaderCircle className="spin" /><p>Opening Bluey Jobs...</p></main>;
}

export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <main className="center-screen">
      <AlertCircle />
      <h1>Jobs did not open</h1>
      <p>{message || "Please try again."}</p>
      <button className="button primary" onClick={onRetry}>Try again</button>
    </main>
  );
}
