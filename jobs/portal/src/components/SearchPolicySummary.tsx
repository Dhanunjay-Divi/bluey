import { CalendarClock, Gauge, ShieldCheck, TrendingUp } from "lucide-react";
import type { CareerProfile } from "../types";
import {
  BLUEY_DAILY_APPLICATION_LIMIT,
  BLUEY_MAX_POSTING_AGE_DAYS,
  roleExperienceAssessment,
} from "../lib/search-policy";

export function SearchPolicySummary({
  profile,
  role = "",
  compact = false,
}: {
  profile: CareerProfile;
  role?: string;
  compact?: boolean;
}) {
  const assessment = roleExperienceAssessment(profile.employment, role);
  const experience = assessment.range;
  const experienceCopy = !role
    ? "Relevant experience only · usually 1 year below to 2 years above"
    : assessment.review_required
      ? `Review required before Bluey sets an experience range for ${role}`
      : experience.years > 0
      ? `${experience.minimum}-${experience.maximum} years for ${role}`
      : `Entry-level ${role} openings until relevant experience is added`;
  return <section className={`search-policy-summary ${compact ? "compact" : ""}`} aria-label="Bluey search policy">
    <div><CalendarClock /><span><b>Recent openings</b><small>Posted within {BLUEY_MAX_POSTING_AGE_DAYS} days and still open</small></span></div>
    <div><TrendingUp /><span><b>Experience fit</b><small>{experienceCopy}</small></span></div>
    <div><Gauge /><span><b>Managed pace</b><small>Up to {BLUEY_DAILY_APPLICATION_LIMIT} applications per day</small></span></div>
    <div><ShieldCheck /><span><b>Review first</b><small>Bluey prepares the kit before any runner starts</small></span></div>
  </section>;
}
