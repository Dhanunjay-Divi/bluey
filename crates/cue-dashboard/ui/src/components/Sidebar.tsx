import { NavLink } from "react-router-dom";
import {
  Home,
  MessageSquare,
  Settings,
  MessageCircle,
  Radio,
  Search,
} from "lucide-react";

const links = [
  { to: "/", icon: Home, label: "Home" },
  { to: "/chats", icon: MessageSquare, label: "Sessions" },
  { to: "/live", icon: Radio, label: "Live" },
  { to: "/responses", icon: MessageCircle, label: "Answers" },
  { to: "/search", icon: Search, label: "Search" },
  { to: "/settings", icon: Settings, label: "Settings" },
];

export function Sidebar() {
  return (
    <nav className="flex w-52 flex-col border-r border-zinc-800 bg-zinc-900 p-3">
      <h1 className="mb-4 px-2 text-lg font-bold text-blue-400">bluey</h1>
      <ul className="flex flex-col gap-1">
        {links.map(({ to, icon: Icon, label }) => (
          <li key={to}>
            <NavLink
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors ${
                  isActive
                    ? "bg-blue-600/20 text-blue-400"
                    : "text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                }`
              }
            >
              <Icon size={16} />
              {label}
            </NavLink>
          </li>
        ))}
      </ul>
      <div className="mt-auto px-2 text-xs leading-5 text-zinc-600">Cmd/Ctrl K command palette</div>
    </nav>
  );
}
