import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { CommandPalette } from "./CommandPalette";
import { BalanceIndicator } from "./BalanceIndicator";
import { AgentModeProvider, useAgentMode } from "../lib/useAgentMode";

// Balance/billing is a managed-AI concept only. In "your agent" mode the user's
// own coding agent answers (no Bluey metering), so the balance pill is hidden.
function ManagedBalance() {
  const { mode } = useAgentMode();
  return mode === "managed" ? <BalanceIndicator /> : null;
}

export function DashboardLayout() {
  return (
    <AgentModeProvider>
      <div className="flex h-screen aurora-bg text-text-primary">
        <Sidebar />
        <main className="relative flex-1 overflow-auto p-6 pt-16">
          <ManagedBalance />
          <Outlet />
        </main>
        <CommandPalette />
      </div>
    </AgentModeProvider>
  );
}
