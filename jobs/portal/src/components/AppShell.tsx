import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";
import {
  Bell,
  BriefcaseBusiness,
  PanelsTopLeft,
  ChevronDown,
  CircleUserRound,
  FileText,
  LayoutDashboard,
  LogOut,
  Moon,
  RefreshCw,
  Settings,
  Sun,
  WalletCards,
} from "lucide-react";
import type { AccountSummary, JobsWorkspace } from "../types";
import { initials, money } from "../lib/format";
import blueyIcon from "../../../../web/assets/bluey-logo.svg";
import blueyWordmark from "../../../../web/assets/bluey-wordmark.svg";

interface Props {
  children: ReactNode;
  account: AccountSummary | null;
  workspace: JobsWorkspace;
  onRefresh: () => void;
  preview: boolean;
}

const navItems = [
  { to: "/matches", label: "Matches", icon: LayoutDashboard },
  { to: "/applications", label: "Applications", icon: BriefcaseBusiness },
  { to: "/resume", label: "Resume", icon: FileText },
  { to: "/browser", label: "Browser", icon: PanelsTopLeft },
  { to: "/settings", label: "Settings", icon: Settings },
];

export function AppShell({ children, account, workspace, onRefresh, preview }: Props) {
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    const saved = localStorage.getItem("bluey_jobs_theme");
    if (saved === "light" || saved === "dark") return saved;
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  });
  const [profileOpen, setProfileOpen] = useState(false);
  const profileRef = useRef<HTMLDivElement>(null);
  const openInterventions = workspace.interventions.filter((item) => item.status === "open").length;
  const destination = (path: string) => `${path}${preview ? "?preview=1" : ""}`;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("bluey_jobs_theme", theme);
  }, [theme]);

  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (!profileRef.current?.contains(event.target as Node)) setProfileOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);

  const signOut = async () => {
    const token = localStorage.getItem("bluey_access_token") || sessionStorage.getItem("bluey_access_token") || "";
    const refresh = localStorage.getItem("bluey_refresh_token") || sessionStorage.getItem("bluey_refresh_token") || "";
    localStorage.removeItem("bluey_access_token");
    localStorage.removeItem("bluey_refresh_token");
    sessionStorage.removeItem("bluey_access_token");
    sessionStorage.removeItem("bluey_refresh_token");
    if (token) {
      void fetch("/auth/logout", {
        method: "POST",
        headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify({ refresh_token: refresh || null }),
      });
    }
    window.location.href = "/";
  };

  return (
    <div className="jobs-shell">
      <header className="app-header">
        <Link className="brand-lockup small" to={destination("/matches")} aria-label="Bluey Jobs home">
          <img className="brand-icon" src={blueyIcon} alt="" />
          <img className="brand-wordmark" src={blueyWordmark} alt="" />
          <b>jobs</b>
        </Link>
        <nav className="desktop-nav" aria-label="Jobs navigation">
          {navItems.map(({ to, label, icon: Icon }) => (
            <NavLink key={to} to={destination(to)} className={({ isActive }) => (isActive ? "active" : "")}>
              <Icon size={16} />{label}
            </NavLink>
          ))}
        </nav>
        <div className="header-actions">
          {preview && <span className="preview-pill">Preview</span>}
          <div className="balance-chip" title="Shared Bluey balance">
            <WalletCards size={15} />
            <span>{money(account?.balance_cents || 0)}</span>
          </div>
          <button className="icon-button" title="Refresh Jobs" onClick={onRefresh}><RefreshCw size={17} /></button>
          <Link className="icon-button notification" title="Intervention inbox" to={destination("/applications")}>
            <Bell size={17} />
            {openInterventions > 0 && <span>{openInterventions}</span>}
          </Link>
          <button
            className="theme-switch"
            title={`Use ${theme === "dark" ? "light" : "dark"} theme`}
            aria-label={`Use ${theme === "dark" ? "light" : "dark"} theme`}
            onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
          >
            <Sun size={14} />
            <span className={theme === "light" ? "light" : "dark"}>{theme === "dark" ? <Moon size={13} /> : <Sun size={13} />}</span>
            <Moon size={14} />
          </button>
          <div className="profile-control" ref={profileRef}>
            <button
              className="profile-button"
              aria-expanded={profileOpen}
              aria-haspopup="menu"
              onClick={() => setProfileOpen((open) => !open)}
            >
              <span>{initials(workspace.profile.full_name)}</span>
              <ChevronDown size={14} />
            </button>
            {profileOpen && (
              <div className="profile-menu" role="menu">
                <div className="profile-summary">
                  <CircleUserRound size={20} />
                  <div><strong>{workspace.profile.full_name}</strong><span>{account?.email || workspace.profile.email}</span></div>
                </div>
                <Link to={destination("/settings")} role="menuitem" onClick={() => setProfileOpen(false)}><Settings size={16} />Jobs settings</Link>
                <a href="/account" role="menuitem"><WalletCards size={16} />Bluey account</a>
                <button role="menuitem" onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}>
                  {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
                  Use {theme === "dark" ? "light" : "dark"} theme
                </button>
                <button role="menuitem" onClick={signOut}><LogOut size={16} />Sign out</button>
              </div>
            )}
          </div>
        </div>
      </header>

      <main className="app-main">{children}</main>

      <nav className="mobile-nav" aria-label="Jobs navigation">
        {navItems.map(({ to, label, icon: Icon }) => (
          <NavLink key={to} to={destination(to)} className={({ isActive }) => (isActive ? "active" : "")}>
            <Icon size={19} /><span>{label}</span>
          </NavLink>
        ))}
      </nav>

      <footer className="app-footer">
        <span>Bluey Jobs beta</span>
        <span>Applications stay tied to the exact resume and answers used.</span>
        <div><a href="/terms">Terms</a><a href="/privacy">Privacy</a><a href="mailto:hello@bluey.sh">Help</a></div>
      </footer>
    </div>
  );
}
