import { NavLink } from "react-router-dom";
import {
  Home,
  MessageSquare,
  Bot,
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
  { to: "/agents", icon: Bot, label: "Agents" },
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
    <nav className="glass flex w-52 flex-col border-r border-hairline p-3">
      <h1 className="mb-4 px-2 text-headline font-semibold tracking-tight text-text-primary">
        bluey
      </h1>
      <ul className="flex flex-col gap-1">
        {links.map(({ to, icon: Icon, label }) => (
          <li key={to}>
            <NavLink
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `flex items-center gap-2 rounded-md px-2 py-1.5 text-subhead transition-colors duration-150 ${
                  isActive
                    ? "bg-accent-subtle text-accent-subtle-text"
                    : "text-text-tertiary hover:bg-bg-raised-2 hover:text-text-secondary"
                }`
              }
            >
              <Icon size={16} />
              {label}
            </NavLink>
          </li>
        ))}
      </ul>
      <div className="mt-auto px-2 text-caption text-text-quaternary">
        &#8984;K command palette
      </div>
    </nav>
  );
}
