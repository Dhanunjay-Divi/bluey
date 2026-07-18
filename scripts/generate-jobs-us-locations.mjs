import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const source = "https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2025_Gazetteer/2025_Gaz_place_national.zip";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "jobs/portal/public/us-locations.json");
const archive = join(tmpdir(), "bluey-2025-us-places.zip");
const statesAndTerritories = {
  AL: "Alabama", AK: "Alaska", AZ: "Arizona", AR: "Arkansas", CA: "California",
  CO: "Colorado", CT: "Connecticut", DE: "Delaware", DC: "District of Columbia",
  FL: "Florida", GA: "Georgia", HI: "Hawaii", ID: "Idaho", IL: "Illinois",
  IN: "Indiana", IA: "Iowa", KS: "Kansas", KY: "Kentucky", LA: "Louisiana",
  ME: "Maine", MD: "Maryland", MA: "Massachusetts", MI: "Michigan", MN: "Minnesota",
  MS: "Mississippi", MO: "Missouri", MT: "Montana", NE: "Nebraska", NV: "Nevada",
  NH: "New Hampshire", NJ: "New Jersey", NM: "New Mexico", NY: "New York",
  NC: "North Carolina", ND: "North Dakota", OH: "Ohio", OK: "Oklahoma", OR: "Oregon",
  PA: "Pennsylvania", RI: "Rhode Island", SC: "South Carolina", SD: "South Dakota",
  TN: "Tennessee", TX: "Texas", UT: "Utah", VT: "Vermont", VA: "Virginia",
  WA: "Washington", WV: "West Virginia", WI: "Wisconsin", WY: "Wyoming",
  AS: "American Samoa", GU: "Guam", MP: "Northern Mariana Islands",
  PR: "Puerto Rico", VI: "U.S. Virgin Islands",
};

execFileSync("curl", ["-fsSL", source, "-o", archive], { stdio: "inherit" });
const text = execFileSync("unzip", ["-p", archive], { encoding: "utf8", maxBuffer: 10 * 1024 * 1024 });
rmSync(archive, { force: true });

const placeLocations = text
  .trim()
  .split(/\r?\n/)
  .slice(1)
  .map((line) => {
    const [state, , , , rawName] = line.split("|");
    const name = rawName
      ?.replace(/\s+(?:city|town|village|borough|municipality|CDP|zona urbana|comunidad)$/i, "")
      .trim();
    return state && name ? `${name}, ${state}` : "";
  })
  .filter(Boolean);
const stateLocations = Object.entries(statesAndTerritories)
  .flatMap(([abbreviation, name]) => [name, abbreviation]);
const locations = [...new Set([...stateLocations, ...placeLocations])]
  .sort((left, right) => left.localeCompare(right));

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, `${JSON.stringify({
  source,
  vintage: 2025,
  count: locations.length,
  locations,
}, null, 2)}\n`);

const generated = JSON.parse(readFileSync(output, "utf8"));
console.log(`Generated ${generated.count} official US place suggestions at ${output}`);
