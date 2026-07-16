import { NavLink } from "react-router-dom";
import {
  BrainCircuit,
  Home,
  MessageSquare,
  Settings,
  MessageCircle,
  Radio,
  Search,
} from "lucide-react";

const links = [
  { to: "/", icon: Home, label: "Home" },
  { to: "/live", icon: Radio, label: "Live" },
  { to: "/context", icon: BrainCircuit, label: "Context" },
  { to: "/chats", icon: MessageSquare, label: "Sessions" },
  { to: "/responses", icon: MessageCircle, label: "Answers" },
  { to: "/search", icon: Search, label: "Search" },
  { to: "/settings", icon: Settings, label: "Settings" },
];

export function Sidebar() {
  return (
    <>
      <nav
        aria-label="Bluey dashboard"
        className="hidden w-56 shrink-0 flex-col border-r border-zinc-800 bg-zinc-900 p-3 lg:flex"
      >
        <NavLink to="/" className="mb-5 flex items-center gap-2 rounded-md px-2 py-2">
          <span className="grid h-8 w-8 place-items-center rounded-lg bg-cyan-400 text-sm font-black text-zinc-950">
            B
          </span>
          <span>
            <span className="block text-base font-bold tracking-tight text-zinc-50">bluey</span>
            <span className="block text-[10px] font-semibold uppercase tracking-[0.14em] text-zinc-600">
              work assistant
            </span>
          </span>
        </NavLink>
        <ul className="flex flex-col gap-1">
          {links.map(({ to, icon: Icon, label }) => (
            <li key={to}>
              <NavLink
                to={to}
                end={to === "/"}
                className={({ isActive }) =>
                  `flex min-h-10 items-center gap-3 rounded-md px-3 text-sm font-medium transition-colors ${
                    isActive
                      ? "bg-cyan-400/10 text-cyan-200"
                      : "text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                  }`
                }
              >
                <Icon aria-hidden="true" size={17} />
                {label}
              </NavLink>
            </li>
          ))}
        </ul>
        <div className="mt-auto rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2 text-xs leading-5 text-zinc-500">
          <kbd className="font-mono text-zinc-300">⌘/Ctrl K</kbd>
          <span className="ml-2">Quick navigation</span>
        </div>
      </nav>

      <nav
        aria-label="Bluey mobile dashboard"
        className="fixed inset-x-3 bottom-3 z-40 grid grid-cols-7 gap-1 rounded-xl border border-zinc-700 bg-zinc-900/95 p-1.5 shadow-2xl shadow-black/50 backdrop-blur lg:hidden"
      >
        {links.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === "/"}
            aria-label={label}
            className={({ isActive }) =>
              `flex min-h-12 min-w-0 flex-col items-center justify-center gap-1 rounded-lg px-1 text-[9px] font-semibold transition-colors ${
                isActive
                  ? "bg-cyan-400/12 text-cyan-200"
                  : "text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
              }`
            }
          >
            <Icon aria-hidden="true" size={17} />
            <span className="max-w-full truncate">{label}</span>
          </NavLink>
        ))}
      </nav>
    </>
  );
}
