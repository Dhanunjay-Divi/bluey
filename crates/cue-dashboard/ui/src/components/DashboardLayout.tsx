import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { CommandPalette } from "./CommandPalette";
import { BalanceIndicator } from "./BalanceIndicator";

export function DashboardLayout() {
  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <Sidebar />
      <main className="relative flex-1 overflow-auto p-6 pt-16">
        <BalanceIndicator />
        <Outlet />
      </main>
      <CommandPalette />
    </div>
  );
}
