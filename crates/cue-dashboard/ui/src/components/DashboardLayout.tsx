import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { CommandPalette } from "./CommandPalette";
import { BalanceIndicator } from "./BalanceIndicator";

export function DashboardLayout() {
  return (
    <div className="flex min-h-screen bg-zinc-950 text-zinc-100">
      <a
        href="#dashboard-main"
        className="skip-link"
      >
        Skip to content
      </a>
      <Sidebar />
      <main
        id="dashboard-main"
        tabIndex={-1}
        className="relative min-w-0 flex-1 overflow-auto px-4 pb-24 pt-16 sm:px-6 lg:pb-8"
      >
        <BalanceIndicator />
        <Outlet />
      </main>
      <CommandPalette />
    </div>
  );
}
