// @ts-nocheck
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  AskHandle,
  AskOptions,
  CalendarConnection,
  ContextItem,
  ContinueResult,
  FixProposal,
  ListeningState,
  MeetingBanner,
  MeetingClient,
  MeetingDecision,
  MeetingState,
  MeetingSummary,
  MeetingViewState,
  SetupStatus,
  SpeakerCandidate,
  TranscriptLine,
} from "./types";

export function createMockClient(): MeetingClient {
  const mockAgents: AgentSummary[] = [
    {
      kind: "claude_code",
      displayName: "Claude Code (CLI)",
      capability: "drive",
      connectorCount: 5,
      readyConnectorCount: 5,
      sessionCount: 20,
      attached: true,
    },
  ];

  const mockConnectors: AgentConnectorInfo[] = [
    { name: "perplexity", authTier: "env_auth", ready: true },
    { name: "github", authTier: "env_auth", ready: true },
    { name: "jira", authTier: "env_auth", ready: true },
    { name: "redis", authTier: "env_auth", ready: true },
  ];

  const mockDecisions: MeetingDecision[] = [
    {
      id: "dec-1",
      text: "Confine participants to a specific time zone to ensure availability",
    },
    {
      id: "dec-2",
      text: "Re-divert domestic network traffic through Toranga and Hamilton gateways",
    },
    {
      id: "dec-3",
      text: "Set 50MB hard limit on Redis payload caching for all unit test runs",
    },
  ];

  // Detailed SVG thumbnail representing Redis Cache Memory & Payload Spikes
  const redisMetricsSvg = `data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="140" height="84" viewBox="0 0 140 84"><rect width="140" height="84" fill="%231c1a19" rx="6"/><text x="10" y="18" fill="%238c867d" font-size="9" font-family="monospace">REDIS CACHE SPIKE</text><path d="M10 65 L30 60 L50 62 L70 30 L90 25 L110 55 L130 50" fill="none" stroke="%23e0443e" stroke-width="2"/><circle cx="90" cy="25" r="3" fill="%23e0443e"/><text x="10" y="78" fill="%234e8d5b" font-size="8" font-family="sans-serif">Payload: 14.2MB (Over Limit)</text></svg>`;

  const mockContext: ContextItem[] = [
    {
      id: "ctx-1",
      title: "screenshot-17849.png",
      kind: "image",
      path: "/Users/ms/Desktop/screenshot-17849.png",
      anchorSegmentId: "seg-2", // Anchored inline early in conversation
      thumbnail: redisMetricsSvg,
    },
    {
      id: "ctx-2",
      title: "redis-cache-limit.patch",
      kind: "code",
      path: "/src/redis/cache_limit.patch",
      anchorSegmentId: "seg-6",
    },
  ];

  const mockTranscript: TranscriptLine[] = [
    {
      id: "seg-1",
      source: "system",
      speaker: "Cr Grahame Webber",
      speakerId: 1,
      text: "Asking internal and external constituents whether I like or don't like about the architectural direction.",
      final: true,
      memberIds: ["seg-1"],
    },
    {
      id: "seg-2",
      source: "system",
      speaker: "Roger Gordon",
      speakerId: 2,
      text: "Yeah, just one final thought on that format? I love that it's a half an hour might almost even take pick particular categories over lengthening the time as an example just because I feel the feeling I have a feeling that if you want to watch it consistently it's got to be in that block.",
      final: true,
      memberIds: ["seg-2"],
    },
    {
      id: "seg-3",
      source: "system",
      speaker: "Cr Lou Brown",
      speakerId: 3,
      text: "but we clearly have gone breath wise we've gone so much broader that it's going to be hard to cover all those topics in a quick session before we dive into the deployment strategy.",
      final: true,
      memberIds: ["seg-3"],
    },
    {
      id: "seg-4",
      source: "system",
      speaker: "Ken Morris",
      speakerId: 4,
      text: "As part our progresses I turn it on for their enormous instance. Have we actually proven it at the world's largest GitLab instance scale?",
      final: true,
      memberIds: ["seg-4"],
    },
    {
      id: "seg-5",
      source: "system",
      speaker: "Cr Andrew Brown",
      speakerId: 5,
      text: "We need a stronger definition of done as part of our progressive delivery. It needs to run at scale and get that dot com successfully without blowing up the cost model not blow up performance.",
      final: true,
      memberIds: ["seg-5"],
    },
    {
      id: "seg-6",
      source: "system",
      speaker: "Marcus Gower",
      speakerId: 6,
      text: "and if it does it should just get immediately reverted frankly um and that should be in the bar for getting features across the line that doesn't mean for new features you know that have low usage.",
      final: true,
      memberIds: ["seg-6"],
    },
    {
      id: "seg-7",
      source: "system",
      speaker: "Cr Bruce Thomas",
      speakerId: 7,
      text: "um I totally agree that you don't want to overbuild on the first iteration for planning for millions of users because that doesn't make any sense but um yeah I think that's one aspect I think their aspect is that on your comic Christopher on pricing.",
      final: true,
      memberIds: ["seg-7"],
    },
    {
      id: "seg-8",
      source: "system",
      speaker: "Cr Mike Pettit",
      speakerId: 8,
      text: "and we can maybe have a follow up here on like a handbook update but um I think it's interesting that customers will absorb the cost on self-managed of compute and so for them if they want to have a ridiculous number of mirrors then fine they're paying for it.",
      final: true,
      memberIds: ["seg-8"],
    },
  ];

  const mockMeetingState: MeetingState = {
    transcript: mockTranscript.map((t) => ({ ...t, id: t.id! })),
    conversation: [
      {
        id: "turn-1",
        // Pinned MID-CONVERSATION right after seg-3 (Cr Lou Brown) so 5 lines flow below it!
        anchorSegmentId: "seg-3",
        question: "Answering the question just asked in the meeting.",
        answer:
          "The last point was Christopher's example about unit test results being cached in Redis with no size limit, causing multi-megabyte payloads and degrading Redis performance. The core takeaway: new features need upfront cost modeling — not exact bytes, but reasonable expectations around scale. When you ship something (like tags or caching), define limits before customers find creative uses that break your infrastructure. Scott was flagged to help drive systematic remediation of the Redis issue.",
        source: "Bluey MCP Engine",
      },
    ],
    decisions: mockDecisions,
    context: mockContext,
  };

  type ListeningSources = { system: boolean; microphone: boolean };
  type ListeningSubscriber = (
    state: ListeningState,
    sources?: ListeningSources,
  ) => void;
  let mockListeningState: ListeningState = "listening";
  let mockListeningSources: ListeningSources = {
    system: true,
    microphone: false,
  };
  const listeningSubscribers = new Set<ListeningSubscriber>();
  const publishListeningState = () => {
    for (const subscriber of listeningSubscribers) {
      subscriber(mockListeningState, { ...mockListeningSources });
    }
  };

  return {
    async listAgents() {
      return mockAgents;
    },
    onAgents(cb) {
      cb(mockAgents);
      return () => {};
    },
    async attach(kind) {
      return mockAgents.map((a) => ({ ...a, attached: a.kind === kind }));
    },
    async detach() {
      return mockAgents.map((a) => ({ ...a, attached: false }));
    },
    async sessions() {
      return [
        {
          id: "s-1",
          title: "Unit Test Cache Fix",
          updatedAt: "Just now",
          project: "/src/redis",
        },
      ];
    },
    async models() {
      return ["auto", "claude-3-5-sonnet", "gpt-4o"];
    },
    async connectors() {
      return mockConnectors;
    },
    async sourceCoverage() {
      return [];
    },
    async setSessionHistoryConsent() {},
    async calendarConnect() {},
    async calendarStatus() {
      return [
        {
          provider: "google",
          configured: true,
          connected: true,
          email: "dev@bluey.ai",
        },
      ];
    },
    async calendarDisconnect() {},
    async meetingState() {
      return mockMeetingState;
    },
    async meetings() {
      return [
        {
          id: "m-1",
          title: "Architecture & Scale Review",
          startedAt: String(Date.now() - 1800000),
          transcriptCount: 24,
          turnCount: 3,
          preview: "Have we actually proven it at scale?",
          isActive: true,
        },
      ];
    },
    async openMeeting(id) {
      return { ...mockMeetingState, meetingId: id, readOnly: true };
    },
    async continueMeeting() {
      return { ok: true, blocked: false };
    },
    newMeeting() {},
    onMeetingReseed(cb) {
      return () => {};
    },
    onMeetingBanner(cb) {
      return () => {};
    },
    respondMeetingPrep() {},
    onTranscript(cb) {
      return () => {};
    },
    onSpeakerUpdate(cb) {
      return () => {};
    },
    renameSpeaker() {},
    reassignSpan() {},
    reassignRange() {},
    splitSegment() {},
    onMeetingCandidates(cb) {
      cb([
        { name: "Cr Grahame Webber", email: "grahame@example.com" },
        { name: "Roger Gordon", email: "roger@example.com" },
        { name: "Cr Lou Brown", email: "lou@example.com" },
        { name: "Ken Morris", email: "ken@example.com" },
        { name: "Cr Andrew Brown", email: "andrew@example.com" },
        { name: "Marcus Gower", email: "marcus@example.com" },
        { name: "Cr Bruce Thomas", email: "bruce@example.com" },
        { name: "Cr Mike Pettit", email: "mike@example.com" },
      ]);
      return () => {};
    },
    onForMeQuestion(cb) {
      // Show the new detected question dock at the bottom!
      cb({
        text: "Should we enforce Redis payload size limits before shipping the tag caching feature?",
        title: "Detected Question",
      });
      return () => {};
    },
    onAgentInstall(cb) {
      return () => {};
    },
    respondAgentInstall() {},
    requestAgentInstall() {},
    requestAgentLogin() {},
    onSetupStatus(cb) {
      cb({
        model: { state: "ready", detail: "Loaded", percent: null, kind: null },
        agent: {
          state: "ready",
          detail: "Claude Code",
          percent: null,
          kind: "claude_code",
        },
        allReady: true,
      });
      return () => {};
    },
    requestSetupStatus() {},
    requestFix(q) {},
    onFixProposal(cb) {
      return () => {};
    },
    ask(question, onChunk) {
      onChunk({
        text: "Analyzing Redis payload cache limits...",
        statusDone: true,
        done: true,
      });
      return { cancel() {} };
    },
    onListeningState(cb) {
      listeningSubscribers.add(cb);
      cb(mockListeningState, { ...mockListeningSources });
      return () => {
        listeningSubscribers.delete(cb);
      };
    },
    startListening(sources) {
      mockListeningSources = {
        microphone: sources?.microphone ?? true,
        system: sources?.system ?? true,
      };
      mockListeningState =
        mockListeningSources.system || mockListeningSources.microphone
          ? "listening"
          : "idle";
      publishListeningState();
    },
    stopListening() {
      mockListeningSources = { system: false, microphone: false };
      mockListeningState = "idle";
      publishListeningState();
    },
    turnOff() {},
    openPermissionSettings() {},
    pickSystemAudio() {},
    capturePage() {},
    openAttachPicker() {},
    captureScreenshot() {},
    onContextItems(cb) {
      cb(mockContext);
      return () => {};
    },
    removeContextItem() {},
    addNote() {},
    toggleSetting() {},
  };
}
