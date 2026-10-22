import type { AdapterTestErrorCode, AdapterTestOperationState, AdapterTestStage, AdapterTestStepState, CodingAgent } from "/assets/shared/contracts.js";
export interface AdapterTestRequest {
  request_id: string;
  adapter_id: string;
  revision: number;
  program: string | null;
  model: string | null;
}
export interface AdapterTestOperation {
  request: AdapterTestRequest;
  state: AdapterTestOperationState;
  sequence: number;
  started_unix_ms: number;
  steps: { stage: AdapterTestStage; state: AdapterTestStepState; elapsed_ms: number }[];
  program: { agent: CodingAgent; executable: string; version: string; digest: string } | null;
  diagnostic: { code: AdapterTestErrorCode; stage: AdapterTestStage; diagnostic_id: string; os_code: number | null; operation: string } | null;
  configuration_revision: number | null;
}
