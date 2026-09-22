/** 媒体库直连模型（非 Agent）开关，与 Agent 模型配置无关，留在这里。 */

export function isDirectResolverReady(input: {
  modelId: string;
  modelBaseUrl: string;
  modelApiKeySaved: boolean;
  pendingApiKey: string;
}): boolean {
  return Boolean(
    input.modelId.trim() &&
      input.modelBaseUrl.trim() &&
      (input.modelApiKeySaved || input.pendingApiKey.trim()),
  );
}

export function isAgentResolverReady(agentModelId: string): boolean {
  return Boolean(agentModelId.trim());
}
