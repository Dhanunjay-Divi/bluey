import {
  ArrowRight,
  BriefcaseBusiness,
  Check,
  Cloud,
  FileText,
  MailCheck,
  Search,
  Sparkles,
} from "lucide-react";
import { loginUrl } from "../api";
import { runnerLandingCopy } from "../lib/runner-access";
import blueyIcon from "../../../../web/assets/bluey-logo.svg";
import blueyWordmark from "../../../../web/assets/bluey-wordmark.svg";

export function AuthGate() {
  return (
    <main className="jobs-entry">
      <header className="entry-header">
        <a className="brand-lockup" href="/" aria-label="Bluey home">
          <img className="brand-icon" src={blueyIcon} alt="" />
          <img className="brand-wordmark" src={blueyWordmark} alt="" />
          <b>jobs</b>
        </a>
        <nav aria-label="Jobs overview">
          <a href="#how-it-works">How it works</a>
          <a href="#jobs-plans">Plans</a>
        </nav>
        <a className="button secondary compact" href={loginUrl()}>Sign in</a>
      </header>

      <section className="entry-hero">
        <div className="entry-hero-copy">
          <p className="eyebrow">BLUEY JOBS</p>
          <h1>Every application,<br /><span>already tailored.</span></h1>
          <p>Give Bluey your profile once. It finds fresh, high-fit roles, creates a unique application kit for each one, and keeps the exact resume and answers together.</p>
          <div className="auth-actions">
            <a className="button primary" href="/login?mode=signup&next=%2Fjobs">Start my job search<ArrowRight size={16} /></a>
            <a className="button secondary" href={loginUrl()}>Sign in to Bluey</a>
          </div>
          <div className="entry-assurances" aria-label="Bluey Jobs defaults">
            <span><Check size={14} />Job-specific resume every time</span>
            <span><Check size={14} />Review first by default</span>
            <span><Check size={14} />Five complete applications free</span>
          </div>
        </div>

        <div className="entry-product-scene" aria-label="Bluey Jobs product preview">
          <header>
            <div><span className="live-dot" /><b>Product engineering</b><small>Career Track active</small></div>
            <span>Review first&nbsp;&nbsp;·&nbsp;&nbsp;Receipt ready</span>
          </header>
          <div className="entry-scene-metrics">
            <span><b>4</b><small>fresh matches</small></span>
            <span><b>89%</b><small>average fit</small></span>
            <span><b>1</b><small>needs review</small></span>
          </div>
          <div className="entry-scene-jobs">
            <div><i>NO</i><span><b>Senior Product Engineer</b><small>Northwind · New York, NY · Posted today</small></span><strong>94%</strong><em>Ready to review</em></div>
            <div><i>AR</i><span><b>Staff Frontend Engineer</b><small>Arcadia Health · Remote, US · Posted yesterday</small></span><strong>91%</strong><em>Needs review</em></div>
            <div><i>AT</i><span><b>Product Engineer, Platform</b><small>Atlas · New York, NY · Posted 5 days ago</small></span><strong>88%</strong><em>Application ready</em></div>
          </div>
          <footer><Sparkles size={15} /><span>Every job keeps its own resume, answers, activity, and submission receipt.</span></footer>
        </div>
      </section>

      <section className="entry-section entry-flow" id="how-it-works">
        <div className="entry-section-heading"><p className="eyebrow">PROFILE TO APPLICATION</p><h2>Set up once. Review the exact packet.</h2><span>Bluey carries your context from the first match through the application receipt.</span></div>
        <ol>
          <li><span><FileText /></span><div><b>Build one Career Profile</b><p>Import your resume, then add work history, locations, preferences, and reusable answers once.</p></div></li>
          <li><span><Search /></span><div><b>Find fresh, relevant roles</b><p>Career Tracks rank recent jobs by role, location, compensation, and your hard filters.</p></div></li>
          <li><span><Sparkles /></span><div><b>Create a unique application kit</b><p>Every job gets its own resume version, answer set, selected email, and visible change summary.</p></div></li>
          <li><span><BriefcaseBusiness /></span><div><b>{runnerLandingCopy.flowTitle}</b><p>{runnerLandingCopy.flowBody}</p></div></li>
          <li><span><MailCheck /></span><div><b>Keep the receipt</b><p>Submission evidence stays tied to the exact resume, answers, application email, and timestamp.</p></div></li>
        </ol>
      </section>

      <section className="entry-section entry-plans" id="jobs-plans">
        <div className="entry-section-heading"><p className="eyebrow">PLANS</p><h2>{runnerLandingCopy.plansTitle}</h2></div>
        <div className="entry-plan-table">
          <div><span><b>Free</b><small>Build your profile and review tailored applications</small></span><strong>$0</strong><p>1 Career Track · 5 complete applications</p></div>
          <div><span><b>Pro</b><small>{runnerLandingCopy.proSummary}</small></span><strong>$29<small>/month</small></strong><p>{runnerLandingCopy.proDetails}</p></div>
          <div><span><b>Cloud</b><small>{runnerLandingCopy.cloudSummary}</small></span><strong>$49<small>/month</small></strong><p>{runnerLandingCopy.cloudDetails}</p></div>
        </div>
        <div className="entry-plan-action"><Cloud size={18} /><span>{runnerLandingCopy.accessNote}</span><a className="button primary" href="/login?mode=signup&next=%2Fjobs">Start free<ArrowRight size={16} /></a></div>
      </section>

      <footer className="entry-footer">
        <span>Bluey Jobs</span>
        <p>One Career Profile. A separate application for every job.</p>
        <nav><a href="/terms">Terms</a><a href="/privacy">Privacy</a><a href="mailto:hello@bluey.sh">Help</a></nav>
      </footer>
    </main>
  );
}
