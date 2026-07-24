import type {
  CareerProfile,
  EducationEntry,
  EmploymentEntry,
  ProjectEntry,
} from "../../types";
import type { ImportedResume } from "./import";

type ResumeSection =
  | "preamble"
  | "summary"
  | "employment"
  | "education"
  | "skills"
  | "certifications"
  | "projects"
  | "other";

interface ResumeSections extends Record<ResumeSection, string[]> {}

export function inferProfileFromResume(profile: CareerProfile, imported: ImportedResume): CareerProfile {
  const sections = splitResumeSections(imported.text);
  const contactLines = sections.preamble.length
    ? sections.preamble
    : normalizeResumeLines(imported.text).slice(0, 12);
  const likelyName = inferName(contactLines);
  const email = imported.text.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i)?.[0];
  const phone = imported.text.match(/(?:\+?1[\s.-]?)?\(?\d{3}\)?[\s.-]\d{3}[\s.-]\d{4}/)?.[0];
  const linkedin = imported.text.match(/(?:https?:\/\/)?(?:www\.)?linkedin\.com\/in\/[^\s|,;]+/i)?.[0];
  const urls = imported.text.match(/https?:\/\/[^\s|,;]+/gi) || [];
  const portfolio =
    urls.find((url) => !/linkedin\.com/i.test(url)) ||
    imported.text.match(/(?:https?:\/\/)?(?:www\.)?(?:github|gitlab)\.com\/[^\s|,;]+/i)?.[0];
  const location = contactLines.map(extractLocation).find(Boolean);
  const inferredEmployment = parseEmployment(sections.employment);
  const inferredEducation = parseEducation(sections.education);
  const inferredProjects = parseProjects(sections.projects);
  const inferredSkills = parseListSection(sections.skills);
  const inferredCertifications = parseCertificationSection(sections.certifications);
  const headline = inferHeadline(sections.preamble, likelyName);
  const summary = sections.summary.map(stripBullet).join(" ").trim();
  return {
    ...profile,
    full_name: profile.full_name || likelyName || "",
    email: profile.email || email || "",
    phone: profile.phone || phone || "",
    headline: profile.headline || headline || inferredEmployment[0]?.title || "",
    current_location:
      profile.current_location ||
      location ||
      inferredEmployment.find((entry) => entry.current && entry.location)?.location ||
      inferredEmployment[0]?.location ||
      "",
    summary: profile.summary || summary,
    linkedin_url: profile.linkedin_url || normalizeUrl(linkedin),
    portfolio_url: profile.portfolio_url || normalizeUrl(portfolio),
    skills: profile.skills?.length ? profile.skills : inferredSkills,
    certifications: profile.certifications?.length
      ? profile.certifications
      : inferredCertifications,
    employment: profile.employment?.length ? profile.employment : inferredEmployment,
    education: profile.education?.length ? profile.education : inferredEducation,
    projects: profile.projects?.length ? profile.projects : inferredProjects,
    source_resume_name: imported.name,
    source_resume_text: imported.text,
  };
}

function splitResumeSections(text: string): ResumeSections {
  const sections: ResumeSections = {
    preamble: [],
    summary: [],
    employment: [],
    education: [],
    skills: [],
    certifications: [],
    projects: [],
    other: [],
  };
  let current: ResumeSection = "preamble";
  for (const line of normalizeResumeLines(text)) {
    const detected = detectSection(line, current);
    if (detected) {
      current = detected.section;
      if (detected.remainder) sections[current].push(detected.remainder);
      continue;
    }
    sections[current].push(line);
  }
  return sections;
}

function normalizeResumeLines(text: string): string[] {
  return text
    .replace(/\u00a0/g, " ")
    .replace(/[●▪◦‣]/g, "•")
    .split(/\r?\n/)
    .map((line) => line.replace(/[ \t]+/g, " ").trim())
    .filter(Boolean);
}

function detectSection(
  line: string,
  current?: ResumeSection,
): { section: ResumeSection; remainder: string } | null {
  if (current === "projects" && /^(?:technologies|tech|stack)\s*:\s*.+/i.test(line)) {
    return null;
  }
  if (
    current === "skills" &&
    /^(?:languages?|ai(?:\/ml)?|machine learning|backend|frontend|cloud(?:\/devops)?|devops|databases?|tools?|frameworks?|platforms?|technologies)\s*:\s*.+/i.test(line)
  ) {
    return null;
  }
  const aliases: Array<[ResumeSection, RegExp]> = [
    ["summary", /^(?:professional\s+)?(?:summary|profile|objective|about)(?:\s*[:|-]\s*(.*))?$/i],
    ["employment", /^(?:(?:professional|work|project|relevant)\s+)?(?:experience|employment)(?:\s*[:|-]\s*(.*))?$/i],
    ["employment", /^(?:work history|career history)(?:\s*[:|-]\s*(.*))?$/i],
    ["education", /^(?:education|academic background|academics)(?:\s*[:|-]\s*(.*))?$/i],
    ["skills", /^(?:technical\s+)?(?:skills|core competencies|technologies|expertise)(?:\s*(?:&|and)\s*(?:interests?|tools?|technologies))?(?:\s*[:|-]\s*(.*))?$/i],
    ["certifications", /^(?:certifications?|licenses?|credentials)(?:\s*[:|-]\s*(.*))?$/i],
    ["projects", /^(?:selected\s+)?projects?(?:\s*(?:&|and)\s*(?:leadership|research|publications?))?(?:\s*[:|-]\s*(.*))?$/i],
    ["other", /^(?:(?:professional|selected)\s+)?(?:affiliations?|memberships?|awards?|honors?|recognition|achievements?|accomplishments?|publications?|languages?|volunteer(?:ing)?|interests?|references?)(?:\s*[:|-]\s*(.*))?$/i],
  ];
  for (const [section, pattern] of aliases) {
    const match = line.match(pattern);
    if (match) return { section, remainder: match[1]?.trim() || "" };
  }
  return null;
}

function inferName(lines: string[]): string {
  return (
    lines.find((line) => {
      const value = stripContactParts(line);
      const words = value.split(/\s+/).filter(Boolean);
      const isSupportedMononym =
        words.length === 1 &&
        /^[A-Z][A-Z'-]{2,39}$/.test(value) &&
        titleScore(value) === 0 &&
        !/^(?:contact|curriculum|details|profile|resume|vitae)$/i.test(value);
      return (
        value.length >= 3 &&
        value.length <= 60 &&
        /^[A-Za-z][A-Za-z .'-]+$/.test(value) &&
        (words.length >= 2 || isSupportedMononym) &&
        !looksLikeLocation(value) &&
        !detectSection(value)
      );
    })?.trim() || ""
  );
}

function inferHeadline(lines: string[], name: string): string {
  return (
    lines.find((line) => {
      const value = stripContactParts(line);
      return (
        value &&
        value !== name &&
        value.length <= 90 &&
        !looksLikeLocation(value) &&
        !looksLikeContact(value) &&
        titleScore(value) > 0
      );
    }) || ""
  );
}

function parseEmployment(lines: string[]): EmploymentEntry[] {
  const entries = datedBlocks(lines, false).map((block, index) => {
    const augmented = augmentEmploymentBlock(block);
    const parsedCandidates = splitHeaderCandidates(augmented.candidates)
      .map(splitEmploymentCandidate);
    const location = parsedCandidates.map((candidate) => candidate.location).find(Boolean) || "";
    const roleCandidates = uniqueStrings(parsedCandidates.map((candidate) => candidate.value).filter(Boolean));
    const combined = roleCandidates.map(splitCombinedTitleCompany).find(Boolean);
    let title = combined?.title || pickByPositiveScore(roleCandidates, titleScore);
    let company = combined?.company || pickByPositiveScore(
        roleCandidates.filter((candidate) => candidate !== title),
        companyScore,
      );
    if (!title) {
      title = roleCandidates.find((candidate) => candidate !== company && companyScore(candidate) === 0) || "";
    }
    if (!company) {
      company = roleCandidates.find((candidate) => candidate !== title) || "";
    }
    if (!title && roleCandidates.length === 1 && companyScore(roleCandidates[0]) === 0) {
      title = roleCandidates[0];
      company = "";
    }
    return {
      id: stableResumeId("employment", `${company}|${title}|${block.start}|${index}`),
      company,
      title,
      location,
      start_date: normalizeDate(block.start),
      end_date: block.current ? "" : normalizeDate(block.end),
      current: block.current,
      highlights: parseHighlights(augmented.body),
    };
  }).filter((entry) => entry.company || entry.title);
  return mergeDuplicateEmployment(entries);
}

function parseEducation(lines: string[]): EducationEntry[] {
  const blocks = datedBlocks(lines, true);
  const source = blocks.length ? blocks : undatedEducationBlocks(lines);
  return source
    .map((block, index) => {
      const candidates = splitHeaderCandidates([...block.header, block.dateRemainder, ...block.body.slice(0, 2)])
        .flatMap(splitCombinedEducationCandidate)
        .flatMap(splitSchoolLocationCandidate)
        .map(cleanEducationValue)
        .filter(Boolean);
      const school = pickByScore(candidates, schoolScore);
      const nonSchoolCandidates = candidates.filter((candidate) => candidate !== school);
      const degreeLine =
        pickByPositiveScore(nonSchoolCandidates, degreeScore) ||
        nonSchoolCandidates.find((candidate) => !looksLikeLocation(candidate)) ||
        "";
      const field =
        degreeLine.match(/[—–-]\s*(.+)$/)?.[1]?.trim() ||
        degreeLine.match(/\bin\s+(.+)$/i)?.[1]?.trim() ||
        degreeLine.match(/^(?:a\.?a\.?|a\.?s\.?|b\.?a\.?|b\.?s\.?|m\.?a\.?|m\.?s\.?)\s+(?:in\s+)?(.+)$/i)?.[1]?.trim() ||
        degreeLine.match(/^(?:masters?|bachelors?|doctorate|ph\.?d\.?)\s*:\s*(.+)$/i)?.[1]?.trim() ||
        degreeLine.match(/^(?:associate|bachelor|master|doctor)(?:'s|s)?(?:\s+(?:degree|of\s+[^,]+))?,\s*(.+)$/i)?.[1]?.trim() ||
        "";
      const location = candidates.find((candidate) => (
        candidate !== school && (looksLikeLocation(candidate) || isCountryName(candidate))
      )) || "";
      return {
        id: stableResumeId("education", `${school}|${degreeLine}|${block.end}|${index}`),
        school,
        degree: degreeLine,
        field,
        start_date: normalizeDate(block.start),
        end_date: normalizeDate(block.end || block.start),
        location,
      };
    })
    .filter((entry) => entry.school || entry.degree);
}

function parseProjects(lines: string[]): ProjectEntry[] {
  const projects: ProjectEntry[] = [];
  let current: { name: string; details: string[] } | null = null;
  const finish = () => {
    if (!current?.name) return;
    const details = joinWrappedLines(current.details.map(stripBullet).filter(Boolean));
    const technologyLine = details.find((line) => /^(?:technologies|tech|stack)\s*:/i.test(line));
    const url = details.join(" ").match(/https?:\/\/[^\s|,;]+/i)?.[0] || "";
    projects.push({
      id: stableResumeId("project", `${current.name}|${projects.length}`),
      name: current.name,
      role: "",
      summary: details
        .filter((line) => line !== technologyLine && (!url || !line.includes(url)))
        .join(" "),
      technologies: technologyLine
        ? parseDelimitedList(technologyLine.replace(/^[^:]+:/, ""))
        : [],
      url,
    });
  };
  for (const line of lines) {
    const clean = stripBullet(line);
    const heading =
      !isBullet(line) &&
      clean.length <= 90 &&
      !/[.!?]$/.test(clean) &&
      !/^(?:technologies|tech|stack)\s*:/i.test(clean) &&
      !/^https?:\/\//i.test(clean);
    if (heading) {
      finish();
      current = { name: clean.replace(/\s*[|—–-]\s*\d{4}.*$/, ""), details: [] };
    } else if (current) {
      current.details.push(clean);
    }
  }
  finish();
  return projects.filter((project) => project.name);
}

function parseListSection(lines: string[]): string[] {
  return repairWrappedListFragments(uniqueStrings(
    lines.flatMap((line) => {
      const value = stripListCategoryPrefix(stripBullet(line));
      return parseDelimitedList(value);
    }),
  )).filter((value) => (
    value.length <= 100 &&
    !/^(?:and|with)$/i.test(value) &&
    !isListCategory(value)
  ));
}

function parseCertificationSection(lines: string[]): string[] {
  const suffix = /^(?:professional|specialty|associate|expert|foundational|practitioner|level\s+(?:i|ii|iii|1|2|3))$/i;
  return uniqueStrings(
    lines
      .flatMap((line) => {
        const value = stripBullet(line).replace(/^[A-Za-z &/+.-]{2,30}:\s*/, "");
        return parseDelimitedList(value);
      })
      .reduce<string[]>((certifications, value) => {
        const clean = value.trim();
        if (!clean) return certifications;
        if (suffix.test(clean) && certifications.length) {
          certifications[certifications.length - 1] = `${certifications.at(-1)}, ${clean}`;
        } else {
          certifications.push(clean);
        }
        return certifications;
      }, []),
  ).filter((value) => value.length <= 120);
}

interface DatedBlock {
  header: string[];
  dateRemainder: string;
  body: string[];
  start: string;
  end: string;
  current: boolean;
}

function datedBlocks(lines: string[], allowSingleYear: boolean): DatedBlock[] {
  const dates = lines
    .map((line, index) => ({ index, range: extractDateRange(line, allowSingleYear) }))
    .filter((item): item is { index: number; range: NonNullable<ReturnType<typeof extractDateRange>> } => (
      Boolean(item.range) && (allowSingleYear || !isNestedAssignmentLine(item.index >= 0 ? lines[item.index] : ""))
    ));
  if (!dates.length) return [];
  const firstInlineRemainder = lines[dates[0].index]
    .replace(dates[0].range.raw, "")
    .replace(/^[|,; -]+|[|,; -]+$/g, "")
    .trim();
  const inlineEducationDateStartsBlock =
    allowSingleYear && Boolean(firstInlineRemainder) && dates[0].index === 0;
  const headerStarts = dates.map(({ index }, dateIndex) => {
    const inlineRemainder = lines[index]
      .replace(dates[dateIndex].range.raw, "")
      .replace(/^[|,; -]+|[|,; -]+$/g, "")
      .trim();
    if (allowSingleYear && inlineRemainder && inlineEducationDateStartsBlock) {
      return index;
    }
    const lowerBound = dateIndex ? dates[dateIndex - 1].index + 1 : 0;
    let start = index;
    while (start > lowerBound && index - start < 3) {
      const candidate = lines[start - 1];
      if (
        isBullet(candidate) ||
        isUsefulHighlight(candidate) ||
        extractDateRange(candidate, allowSingleYear)
      ) break;
      start -= 1;
    }
    return start;
  });
  return dates.map(({ index, range }, dateIndex) => ({
    header: lines.slice(headerStarts[dateIndex], index),
    dateRemainder: lines[index].replace(range.raw, "").replace(/^[|,; -]+|[|,; -]+$/g, "").trim(),
    body: lines.slice(index + 1, headerStarts[dateIndex + 1] ?? lines.length),
    start: range.start,
    end: range.end,
    current: /present|current|now/i.test(range.end),
  }));
}

function undatedEducationBlocks(lines: string[]): DatedBlock[] {
  const schoolIndexes = lines
    .map((line, index) => ({ line, index }))
    .filter(({ line }) => schoolScore(line) > 0);
  return schoolIndexes.map(({ index }, itemIndex) => ({
    header: lines.slice(index, schoolIndexes[itemIndex + 1]?.index ?? lines.length),
    dateRemainder: "",
    body: [],
    start: "",
    end: lines.slice(index, schoolIndexes[itemIndex + 1]?.index ?? lines.length)
      .join(" ")
      .match(/\b(?:19|20)\d{2}\b/g)
      ?.at(-1) || "",
    current: false,
  }));
}

function extractDateRange(
  line: string,
  allowSingleYear = false,
): { raw: string; start: string; end: string } | null {
  const month = "(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)";
  const point = `(?:${month}\\s+)?(?:19|20)\\d{2}`;
  const match = line.match(new RegExp(`(${point})\\s*(?:-|–|—|to)\\s*(${point}|Present|Current|Now)`, "i"));
  if (match) return { raw: match[0], start: match[1], end: match[2] };
  const years = line.match(/\b(?:19|20)\d{2}\b/g);
  if (allowSingleYear && years?.length === 1 && stripBullet(line).length <= 80) {
    return { raw: years[0], start: "", end: years[0] };
  }
  return null;
}

function normalizeDate(value: string): string {
  const trimmed = value.trim();
  if (!trimmed || /present|current|now/i.test(trimmed)) return "";
  const year = trimmed.match(/\b(?:19|20)\d{2}\b/)?.[0] || "";
  const monthName = trimmed.match(/[A-Za-z]+/)?.[0]?.slice(0, 3).toLowerCase();
  const monthIndex = monthName
    ? ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"].indexOf(monthName)
    : -1;
  return year && monthIndex >= 0 ? `${year}-${String(monthIndex + 1).padStart(2, "0")}` : year;
}

function splitHeaderCandidates(lines: string[]): string[] {
  return uniqueStrings(
    lines.flatMap((line) => {
      const clean = stripDanglingDateMonth(stripBullet(line));
      const separator = degreeScore(clean) > 0
        ? /\s*[|•]\s*/i
        : schoolScore(clean) > 0
          ? /\s*[|•]\s*|\s+[—–]\s+/i
          : /\s+(?:at|@)\s+|\s*[|•]\s*|\s+[—–]\s+/i;
      return clean
        .split(separator)
        .map((part) => part.trim())
        .filter(Boolean);
    }),
  );
}

function splitEmploymentCandidate(candidate: string): { value: string; location: string } {
  const clean = candidate.replace(/[.,]+$/, "").trim();
  if (!clean) return { value: "", location: "" };
  if (/^(?:remote|hybrid|on-?site)(?:\s*[-–—]\s*.+)?$/i.test(clean)) {
    return { value: "", location: clean };
  }
  const parts = clean.split(/\s*,\s*/).map((part) => part.trim()).filter(Boolean);
  if (parts.length < 2) return { value: clean, location: "" };

  const last = parts.at(-1) || "";
  if (isUsStateCode(last) || isUsStateName(last)) {
    if (parts.length === 2) {
      const split = splitCompanyAndCity(parts[0]);
      if (split) {
        return { value: split.company, location: `${split.city}, ${last}` };
      }
    }
    return {
      value: parts.slice(0, -2).join(", "),
      location: parts.slice(-2).join(", "),
    };
  }
  if (/\b(?:remote|hybrid|on-?site)\b/i.test(last)) {
    return { value: parts.slice(0, -1).join(", "), location: last };
  }
  if (isCountryName(last)) {
    if (parts.length === 2) {
      const split = splitCompanyAndCity(parts[0]);
      if (split) {
        return { value: split.company, location: `${split.city}, ${last}` };
      }
    }
    return {
      value: parts.slice(0, -2).join(", "),
      location: parts.slice(-2).join(", "),
    };
  }
  return { value: clean, location: "" };
}

function splitCombinedTitleCompany(candidate: string): { title: string; company: string } | null {
  const parts = candidate.split(/\s*,\s*/).map((part) => part.trim()).filter(Boolean);
  if (parts.length < 2) return null;
  const legalSuffix = /^(?:inc\.?|llc|ltd\.?|corp\.?|plc|co\.?)$/i;
  const scored = Array.from({ length: parts.length - 1 }, (_, offset) => {
    const boundary = offset + 1;
    const left = parts.slice(0, boundary).join(", ");
    const right = parts.slice(boundary).join(", ");
    const candidates = [
      { title: left, company: right, titleFirst: true },
      { title: right, company: left, titleFirst: false },
    ];
    return candidates.map((value) => ({
      ...value,
      boundary,
      titleScore: titleScore(value.title),
      companyScore: companyScore(value.company),
      companyTitleScore: titleScore(value.company),
      companyParts: value.company.split(/\s*,\s*/).filter(Boolean).length,
      legalOnly: legalSuffix.test(value.company),
    }));
  })
    .flat()
    .filter((value) => (
      value.titleScore > 0 &&
      value.company &&
      (value.companyTitleScore === 0 || (
        value.companyScore > 0 && value.titleScore > value.companyTitleScore
      ))
    ))
    .sort((left, right) => {
      if (left.legalOnly !== right.legalOnly) return left.legalOnly ? 1 : -1;
      if (left.companyScore !== right.companyScore) return right.companyScore - left.companyScore;
      if (left.companyParts !== right.companyParts) return left.companyParts - right.companyParts;
      if (left.titleFirst !== right.titleFirst) {
        return left.titleFirst ? right.boundary - left.boundary : left.boundary - right.boundary;
      }
      return right.titleScore - left.titleScore;
    });
  const best = scored[0];
  return best ? { title: best.title, company: best.company } : null;
}

function pickByScore(values: string[], score: (value: string) => number): string {
  return values
    .map((value, index) => ({ value, index, score: score(value) }))
    .sort((left, right) => right.score - left.score || left.index - right.index)[0]?.value || "";
}

function pickByPositiveScore(values: string[], score: (value: string) => number): string {
  const selected = values
    .map((value, index) => ({ value, index, score: score(value) }))
    .filter((candidate) => candidate.score > 0)
    .sort((left, right) => right.score - left.score || left.index - right.index)[0];
  return selected?.value || "";
}

function titleScore(value: string): number {
  return keywordScore(value, [
    "engineer", "developer", "manager", "director", "analyst", "scientist", "designer",
    "consultant", "specialist", "architect", "lead", "intern", "associate", "coordinator",
    "product", "research", "operations", "founder", "president", "officer", "nurse",
    "physician", "clinician", "technician", "administrator", "assistant", "researcher",
    "pharmacist", "dentist", "therapist", "recruiter", "counsel", "accountant", "paa",
  ]);
}

function companyScore(value: string): number {
  return keywordScore(value, [
    "inc", "llc", "ltd", "corp", "company", "group", "labs", "technologies", "systems",
    "solutions", "consulting", "bank", "university", "health", "media",
    "hospital", "hospitals", "medical", "clinic", "care", "system", "center", "centre",
    "insurance", "manufacturing", "manufacturers", "library", "communications", "telecom",
    "foundation", "association",
  ]);
}

function schoolScore(value: string): number {
  return keywordScore(value, ["university", "college", "institute", "school", "academy", "polytechnic"]);
}

function degreeScore(value: string): number {
  return keywordScore(value, [
    "bachelor", "master", "doctor", "phd", "mba", "b.s", "b.a", "m.s", "m.a",
    "associate", "diploma", "certificate",
  ]);
}

function keywordScore(value: string, keywords: string[]): number {
  const normalized = value.toLowerCase();
  return keywords.reduce((score, keyword) => score + (normalized.includes(keyword) ? 1 : 0), 0);
}

function stripBullet(line: string): string {
  return line.replace(/^\s*(?:[-*•]\s*)+/, "").trim();
}

function isBullet(line: string): boolean {
  return /^\s*[-*•]/.test(line);
}

const EMPLOYMENT_NARRATIVE_START_PATTERN = /^(?:achieved|administered|analyzed|assisted|built|collaborated|coordinated|created|delivered|designed|developed|directed|drove|established|executed|implemented|improved|increased|launched|led|managed|optimized|owned|reduced|supported|trained|verified|worked)\b/i;

function isUsefulHighlight(line: string): boolean {
  const value = stripBullet(line);
  return (
    isBullet(line) ||
    value.length > 90 ||
    /[.!?]$/.test(value) ||
    EMPLOYMENT_NARRATIVE_START_PATTERN.test(value)
  );
}

function looksLikeEmploymentNarrative(line: string): boolean {
  const value = stripBullet(line).trim();
  if (!value) return false;
  if (isBullet(line) || EMPLOYMENT_NARRATIVE_START_PATTERN.test(value)) return true;
  return value.split(/\s+/).length >= 12 && /[.!?]$/.test(value);
}

function looksLikeContact(line: string): boolean {
  return /@|https?:\/\/|linkedin\.com|\+?\d[\d\s().-]{8,}/i.test(line);
}

function stripContactParts(line: string): string {
  return line
    .replace(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi, "")
    .replace(/(?:\+?1[\s.-]?)?\(?\d{3}\)?[\s.-]\d{3}[\s.-]\d{4}/g, "")
    .replace(/https?:\/\/\S+/gi, "")
    .replace(/\s*[|•—–]\s*/g, " ")
    .trim();
}

function looksLikeLocation(line: string): boolean {
  const value = stripContactParts(line);
  if (new RegExp(`^[A-Za-z .'-]+,\\s*(?:${US_STATE_CODE_PATTERN}|${US_STATE_NAME_PATTERN}|${COUNTRY_PATTERN})$`, "i").test(value)) {
    return true;
  }
  const parsed = splitEmploymentCandidate(value);
  return Boolean(parsed.location) && !parsed.value;
}

function extractLocation(line: string): string {
  const location = (
    line
      .split(/\s*[|•—–]\s*/)
      .map(stripContactParts)
      .find((part) => part && looksLikeLocation(part)) || ""
  );
  return normalizeLocationValue(location);
}

function parseHighlights(lines: string[]): string[] {
  const highlights: string[] = [];
  let current = "";
  let pendingLabel = "";
  const finish = () => {
    if (current) highlights.push(current.trim());
    current = "";
  };
  for (const line of lines) {
    const value = stripBullet(line);
    if (!value) continue;
    if (isNestedAssignmentLine(value)) {
      finish();
      highlights.push(value);
      pendingLabel = "";
      continue;
    }
    if (isBullet(line)) {
      finish();
      current = pendingLabel ? `${pendingLabel}: ${value}` : value;
      pendingLabel = "";
      continue;
    }
    if (isHighlightSubheading(value, current)) {
      finish();
      pendingLabel = value;
      continue;
    }
    if (!current) {
      if (isUsefulHighlight(line)) current = value;
      continue;
    }
    if (/[.!?]$/.test(current) && isUsefulHighlight(line)) {
      finish();
      current = value;
    } else {
      current = joinWrappedText(current, value);
    }
  }
  finish();
  return uniqueStrings(highlights);
}

function isHighlightSubheading(value: string, current: string): boolean {
  const heading = value.replace(/\s+\([A-Z][A-Z0-9&/-]{1,10}\)\s*$/, "");
  const words = heading.split(/\s+/).filter(Boolean);
  const titleLike = words.every((word) =>
    /^(?:and|for|of|the|to|with|&|[-–—])$/i.test(word) ||
    /^[A-Z0-9][A-Za-z0-9/&+.'-]*$/.test(word),
  );
  const headingContext = /\b(?:administration|clinical|engineering|health|information|leadership|management|operations|project|research|technical)\b/i.test(heading);
  return (
    value.length >= 3 &&
    value.length <= 80 &&
    !/[.!?]$/.test(value) &&
    !extractDateRange(value) &&
    !looksLikeContact(value) &&
    titleLike &&
    headingContext &&
    (!current || /[.!?]$/.test(current))
  );
}

function joinWrappedLines(lines: string[]): string[] {
  const joined: string[] = [];
  for (const line of lines) {
    if (!line) continue;
    if (!joined.length || /^(?:technologies|tech|stack)\s*:/i.test(line)) {
      joined.push(line);
      continue;
    }
    const previous = joined[joined.length - 1];
    if (/[.!?]$/.test(previous)) joined.push(line);
    else joined[joined.length - 1] = joinWrappedText(previous, line);
  }
  return joined;
}

function joinWrappedText(left: string, right: string): string {
  return /[A-Za-z]-$/.test(left)
    ? `${left.slice(0, -1)}${right}`
    : `${left} ${right}`;
}

function parseDelimitedList(value: string): string[] {
  const parts: string[] = [];
  let current = "";
  let depth = 0;
  for (const character of value) {
    if (character === "(" || character === "[") depth += 1;
    if (character === ")" || character === "]") depth = Math.max(0, depth - 1);
    if (depth === 0 && /[|,;•]/.test(character)) {
      if (current.trim()) parts.push(cleanListValue(current));
      current = "";
      continue;
    }
    current += character;
  }
  if (current.trim()) parts.push(cleanListValue(current));
  if (parts.length > 1) return parts;
  return value.split(/\s{2,}/).map(cleanListValue).filter(Boolean);
}

function augmentEmploymentBlock(block: DatedBlock): { candidates: string[]; body: string[] } {
  const candidates = [...block.header, block.dateRemainder];
  const consumed = new Set<number>();
  for (let index = 0; index < Math.min(block.body.length, 5); index += 1) {
    const line = block.body[index];
    const clean = stripBullet(line).replace(/[.]$/, "").trim();
    if (!clean || isNestedAssignmentLine(clean) || /^(?:responsibilities|duties)\s*:?$/i.test(clean)) {
      continue;
    }
    if (looksLikeEmploymentNarrative(line)) break;
    const parsed = splitEmploymentCandidate(clean);
    const headerLike = (
      !isBullet(line) &&
      clean.length <= 120 &&
      (titleScore(parsed.value) > 0 || companyScore(parsed.value) > 0 || Boolean(parsed.location))
    );
    if (!headerLike) break;
    candidates.push(clean);
    consumed.add(index);
  }
  return {
    candidates,
    body: block.body.filter((_, index) => !consumed.has(index)),
  };
}

function mergeDuplicateEmployment(entries: EmploymentEntry[]): EmploymentEntry[] {
  const merged = new Map<string, EmploymentEntry>();
  for (const entry of entries) {
    const key = [
      entry.company,
      entry.title,
      entry.start_date,
      entry.end_date,
      String(entry.current),
    ].map((value) => value.trim().toLowerCase()).join("|");
    const existing = merged.get(key);
    if (!existing) {
      merged.set(key, entry);
      continue;
    }
    existing.location ||= entry.location;
    existing.highlights = uniqueStrings([...existing.highlights, ...entry.highlights]);
  }
  return [...merged.values()];
}

function splitCompanyAndCity(value: string): { company: string; city: string } | null {
  const words = value.split(/\s+/).filter(Boolean);
  if (words.length < 2) return null;
  const suffix = /^(?:academy|association|bank|capital|care|center|centre|clinic|college|communications|consulting|corp(?:oration)?|foundation|group|health|hospital|hospitals|insurance|labs?|library|manufacturing|medical|services|solutions|systems|technologies|technology|telecom|university)$/i;
  let suffixIndex = -1;
  words.forEach((word, index) => {
    if (suffix.test(word.replace(/[^A-Za-z]/g, ""))) suffixIndex = index;
  });
  if (suffixIndex >= 0 && suffixIndex < words.length - 1) {
    return {
      company: words.slice(0, suffixIndex + 1).join(" "),
      city: words.slice(suffixIndex + 1).join(" "),
    };
  }
  return { company: words.slice(0, -1).join(" "), city: words.at(-1) || "" };
}

function isNestedAssignmentLine(value: string): boolean {
  return /^(?:client|customer|project|assignment|engagement)\s*:/i.test(stripBullet(value));
}

const LIST_CATEGORIES = [
  "& backend", "ai/ml", "backend", "bpm tools", "cloud", "cloud/devops", "databases", "devops",
  "frameworks", "ide tools", "languages", "messaging & event processing", "messaging systems",
  "operating systems", "platforms", "spring suite", "technical skills", "technologies",
  "tools", "tools & monitoring",
];

function stripListCategoryPrefix(value: string): string {
  if (/^[A-Za-z0-9 &/+.-]{2,40}:\s*$/.test(value)) return "";
  const colon = value.match(/^[A-Za-z0-9 &/+.-]{2,40}:\s*(.+)$/);
  if (colon) return colon[1].trim();
  const normalized = value.toLowerCase();
  const category = LIST_CATEGORIES
    .filter((candidate) => normalized.startsWith(`${candidate} `))
    .sort((left, right) => right.length - left.length)[0];
  return category ? value.slice(category.length).trim() : value;
}

function isListCategory(value: string): boolean {
  return LIST_CATEGORIES.includes(value.trim().replace(/:$/, "").toLowerCase());
}

function cleanListValue(value: string): string {
  const clean = value.trim().replace(/[.:;]+$/, "").trim();
  if (/^Eclipse\s+My\s*Eclipse$/i.test(clean)) return "Eclipse|MyEclipse";
  return clean;
}

function repairWrappedListFragments(values: string[]): string[] {
  const expanded = values.flatMap((value) => value.split("|").map((item) => item.trim()).filter(Boolean));
  const repaired: string[] = [];
  const continuations: Array<[RegExp, RegExp]> = [
    [/^trend$/i, /^analysis$/i],
    [/^ai-enhanced$/i, /^reporting$/i],
    [/^spring$/i, /^batch(?:\b|\s*\()/i],
  ];
  for (const value of expanded) {
    const previous = repaired.at(-1);
    const pair = previous && continuations.some(([left, right]) => left.test(previous) && right.test(value));
    if (pair) repaired[repaired.length - 1] = `${previous} ${value}`;
    else repaired.push(value);
  }
  return uniqueStrings(repaired);
}

function normalizeLocationValue(value: string): string {
  return value
    .replace(/\s*,\s*/g, ", ")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function splitCombinedEducationCandidate(value: string): string[] {
  const clean = value.replace(/[.]$/, "").trim();
  if (degreeScore(clean) === 0 || schoolScore(clean) === 0) return [clean];
  const parts = clean.split(/\s*,\s*/).filter(Boolean);
  const schoolIndex = parts.findIndex((part) => schoolScore(part) > 0);
  if (schoolIndex === 0 && parts.slice(1).some((part) => degreeScore(part) > 0)) {
    return [parts[0], parts.slice(1).join(", ")];
  }
  if (schoolIndex <= 0) return [clean];
  return [
    parts.slice(0, schoolIndex).join(", "),
    parts[schoolIndex],
    ...parts.slice(schoolIndex + 1),
  ].filter(Boolean);
}

function splitSchoolLocationCandidate(value: string): string[] {
  const gpaLocationPattern = new RegExp(
    `^(.+?)\\s*\\(?GPA\\s*:[^)]+\\)?\\s+([A-Za-z .'-]+,\\s*(?:${US_STATE_CODE_PATTERN}|${US_STATE_NAME_PATTERN}|${COUNTRY_PATTERN}))$`,
    "i",
  );
  const gpaLocation = value.trim().match(gpaLocationPattern);
  if (gpaLocation && degreeScore(gpaLocation[1]) > 0) {
    return [cleanEducationValue(gpaLocation[1]), normalizeLocationValue(gpaLocation[2])];
  }
  const clean = cleanEducationValue(value);
  const locationPattern = new RegExp(
    `^(.+?)\\s+([A-Za-z .'-]+,\\s*(?:${US_STATE_CODE_PATTERN}|${US_STATE_NAME_PATTERN}|${COUNTRY_PATTERN}))$`,
    "i",
  );
  const match = clean.match(locationPattern);
  if (match && (schoolScore(match[1]) > 0 || degreeScore(match[1]) > 0)) {
    return [match[1].trim(), normalizeLocationValue(match[2])];
  }
  const countryMatch = clean.match(new RegExp(`^(.+?)\\s+(${COUNTRY_PATTERN})$`, "i"));
  if (countryMatch && (schoolScore(countryMatch[1]) > 0 || degreeScore(countryMatch[1]) > 0)) {
    return [countryMatch[1].trim(), countryMatch[2].trim()];
  }
  return [clean];
}

function cleanEducationValue(value: string): string {
  return value
    .replace(/\[\s*\]/g, "")
    .replace(/\s*\(?GPA\s*:[^)]+\)?/gi, "")
    .replace(/\s*(?:expected\s+)?graduation\s+date\s*:?\s*$/i, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function uniqueStrings(values: string[]): string[] {
  const seen = new Set<string>();
  return values.filter((value) => {
    const normalized = value.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) return false;
    seen.add(normalized);
    return true;
  });
}

function stripDanglingDateMonth(value: string): string {
  return value
    .replace(/,?\s*(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\s*$/i, "")
    .replace(/,+$/, "")
    .trim();
}

function isUsStateCode(value: string): boolean {
  return new RegExp(`^(?:${US_STATE_CODE_PATTERN})(?:\\s+\\d{5}(?:-\\d{4})?)?$`, "i").test(value);
}

function isUsStateName(value: string): boolean {
  return new RegExp(`^(?:${US_STATE_NAME_PATTERN})$`, "i").test(value);
}

function isCountryName(value: string): boolean {
  return new RegExp(`^(?:${COUNTRY_PATTERN})$`, "i").test(value);
}

const US_STATE_CODE_PATTERN = "A[LKSZR]|C[AOT]|D[EC]|F[LM]|G[A]|H[I]|I[ADLN]|K[SY]|L[A]|M[ADEHINOST]|N[CDEHJMVY]|O[HKR]|P[A]|R[I]|S[CD]|T[NX]|U[T]|V[AIT]|W[AIVY]";
const US_STATE_NAME_PATTERN = "Alabama|Alaska|Arizona|Arkansas|California|Colorado|Connecticut|Delaware|Florida|Georgia|Hawaii|Idaho|Illinois|Indiana|Iowa|Kansas|Kentucky|Louisiana|Maine|Maryland|Massachusetts|Michigan|Minnesota|Mississippi|Missouri|Montana|Nebraska|Nevada|New Hampshire|New Jersey|New Mexico|New York|North Carolina|North Dakota|Ohio|Oklahoma|Oregon|Pennsylvania|Rhode Island|South Carolina|South Dakota|Tennessee|Texas|Utah|Vermont|Virginia|Washington|West Virginia|Wisconsin|Wyoming|District of Columbia|Puerto Rico";
const COUNTRY_PATTERN = "Argentina|Australia|Austria|Belgium|Brazil|Canada|Chile|China|Colombia|Denmark|Egypt|Finland|France|Germany|Greece|India|Indonesia|Ireland|Israel|Italy|Japan|Kenya|Malaysia|Mexico|Netherlands|New Zealand|Nigeria|Norway|Pakistan|Philippines|Poland|Portugal|Singapore|South Africa|South Korea|Spain|Sweden|Switzerland|Taiwan|Thailand|Turkey|United Arab Emirates|United Kingdom|United States|Vietnam";

function normalizeUrl(value?: string): string {
  if (!value) return "";
  return /^https?:\/\//i.test(value) ? value : `https://${value}`;
}

function stableResumeId(kind: string, value: string): string {
  let hash = 2166136261;
  for (const character of value) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return `resume-${kind}-${(hash >>> 0).toString(16)}`;
}
