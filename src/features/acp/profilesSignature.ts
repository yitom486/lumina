import type { AgentProfileInput } from "./types";

/** Stable string for React Query keys — avoids subscribing to profile arrays. */
export function profilesSignature(profiles: AgentProfileInput[]): string {
  return profiles
    .map(
      (profile) =>
        `${profile.id}|${profile.command}|${(profile.args ?? []).join(",")}`,
    )
    .join(";");
}
