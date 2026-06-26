# Round 193 - GSC Bing Owner Submission Soft Launch

Date: 2026-06-26  
Continuity thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked to take laptop control and finish the previously blocked owner-account actions for Google/Bing submission and public posts, without pretending account-gated items were complete.

## Search Console

- Added and deployed Google verification file:
  - `web/google2e56c521751b801a.html`
  - live: `https://bluey.sh/google2e56c521751b801a.html`
- Verified the Google Search Console URL-prefix property for `https://bluey.sh/` under the owner account.
- Submitted `https://bluey.sh/sitemap.xml`.
- Search Console reported:
  - status `Success`
  - last read `Jun 26, 2026`
  - discovered pages `17`
- Fixed the homepage Product JSON-LD issue by adding a concrete `Offer` to the homepage Product schema.
- Redeployed `web/index.html`.
- Ran a live URL test for `https://bluey.sh/`; Search Console reported the page is available to Google, page can be indexed, and Product snippets/Merchant listings are valid with only non-critical issues.
- Requested Google indexing for the homepage and priority feature pages:
  - `https://bluey.sh/`
  - `https://bluey.sh/how-bluey-works/`
  - `https://bluey.sh/bluey-faq/`
  - `https://bluey.sh/ai-meeting-context-copilot/`
  - `https://bluey.sh/engineering-meeting-copilot/`
  - `https://bluey.sh/screen-context-ai-assistant/`
- A later bulk indexing-request attempt reached the remaining sitemap set but timed out in automation; the final visible `llms.txt` request returned Google's generic `Oops! Something went wrong` submission error. The sitemap still covers all 17 URLs.

## Bing

- Signed into Bing Webmaster Tools through the owner Google account.
- Imported the verified `https://bluey.sh/` property from Google Search Console.
- Confirmed the imported sitemap:
  - sitemap: `https://bluey.sh/sitemap.xml`
  - status `Success`
  - errors `0`
  - warnings `0`
  - URLs discovered `17`
- Submitted all 17 sitemap URLs through Bing URL Submission.
- Bing reported `Success: 17 URLs submitted Successfully`, `URLs submitted today: 17`, and quota left `83`.
- Rechecked Bing IndexNow panel; Bing's in-product page showed the introductory IndexNow panel. Round 192 remains the source of truth for direct IndexNow API submissions, where both IndexNow endpoints returned HTTP `202`.

## Soft Launch Posts

- Published X post from `@vectorTrdr`:
  - `https://x.com/vectorTrdr/status/2070509720645820539`
- Published LinkedIn founder note:
  - `https://www.linkedin.com/feed/update/urn:li:share:7476275795267743746`
- Published Reddit profile post to `u/Suitable-Capital-716`:
  - `https://www.reddit.com/user/Suitable-Capital-716/comments/1ug7zwa/bluey_a_private_desktop_ai_copilot_for/`

## Remaining Gates

- Hacker News was not posted because `https://news.ycombinator.com/submit` required login. Do not create a new HN account in automation.
- Product Hunt was not launched because the browser was signed out and the repo launch calendar says Product Hunt should wait for the polished launch push. It still needs owner sign-in, launch assets, maker/profile decisions, and final launch timing.
- Reddit subreddit posts were not made because the correct subreddit/community and moderation rules were not specified. The safe Reddit action this round was the owner-profile post.
- Slack/Discord/community posts remain owner/community-specific because workspace membership, permissions, and moderator expectations vary.
- Search indexing still needs 24-72 hour monitoring in Google Search Console and Bing Webmaster Tools.

## Verification

- `curl -fsS https://bluey.sh/google2e56c521751b801a.html`
- `curl -fsS https://bluey.sh/sitemap.xml | rg -c '<loc>'` returned `17`.
- `curl -fsS https://bluey.sh/llms.txt` returned the AI discovery summary.
- `curl -fsS https://bluey.sh/ | rg -n '"offers"|"price"|AI Meeting Context Copilot'` confirmed the live homepage title and schema offers.
- Bing Webmaster Tools UI confirmed sitemap success and 17 manual URL submissions.
- Google Search Console UI confirmed property verification, sitemap success, homepage live test success, and priority indexing requests.

## Files

- `web/google2e56c521751b801a.html`
- `web/index.html`
- `docs/marketing/BLUEY-SEARCH-SUBMISSION-PACK-20260626.md`
- `docs/rounds/ROUND-193-GSC-BING-OWNER-SUBMISSION-SOFT-LAUNCH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`
