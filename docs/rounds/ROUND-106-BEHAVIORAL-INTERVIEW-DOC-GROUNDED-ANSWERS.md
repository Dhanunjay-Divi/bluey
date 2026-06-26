# Round 106 - Behavioral Interview Doc Grounding

## Change
- Added a behavioral interview answer mode when the question asks for an interview story and resume/JD/prep context is attached.
- The mode asks Bluey to produce a complete first-person answer the user can say aloud, using attached documents as source material.
- The answer is shaped with STAR internally: situation, task, action, result, without forcing visible labels unless useful.

## Product Intent
When a user attaches a resume, JD, or interview prep doc and asks something like "tell me about a time you worked under pressure," Bluey should not give a short generic answer. It should use the attached context and produce a complete ready-to-say response.

## Guardrails
- Do not invent metrics, employers, or ownership not supported by the attached context.
- If no confirmed metric exists, use a qualitative result.
- Keep the answer speakable and avoid em dashes.
