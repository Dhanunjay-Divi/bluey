import { NavLink } from "react-router-dom";
import {
  Home,
  MessageSquare,
  FileText,
  Keyboard,
  Settings,
  MessageCircle,
  Camera,
  Mic,
  Code,
  Radio,
  Search,
} from "lucide-react";

const links = [
  { to: "/", icon: Home, label: "Home" },
  { to: "/chats", icon: MessageSquare, label: "Chats" },
  { to: "/prompts", icon: FileText, label: "Prompts" },
  { to: "/shortcuts", icon: Keyboard, label: "Shortcuts" },
  { to: "/settings", icon: Settings, label: "Settings" },
  { to: "/responses", icon: MessageCircle, label: "Responses" },
  { to: "/screenshot", icon: Camera, label: "Screenshot" },
  { to: "/audio", icon: Mic, label: "Audio" },
  { to: "/live", icon: Radio, label: "Live" },
  { to: "/search", icon: Search, label: "Search" },
  { to: "/dev", icon: Code, label: "Dev Tools" },
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
      <div className="mt-auto px-2 text-xs text-zinc-600">⌘K command palette</div>
    </nav>
  );
}
