import type { BindingErrorCode, BindingOperationState, BindingStage, BindingStepState, CodingAgent } from "/assets/shared/contracts.js";
export interface BindingRequest {
  request_id: string;
  person_id: string;
  adapter_id: string;
  revision: number;
  program: string | null;
  model: string | null;
}
export interface BindingOperation {
  request: BindingRequest;
  state: BindingOperationState;
  sequence: number;
  started_unix_ms: number;
  steps: { stage: BindingStage; state: BindingStepState; elapsed_ms: number }[];
  program: { agent: CodingAgent; executable: string; version: string; digest: string } | null;
  diagnostic: { code: BindingErrorCode; stage: BindingStage; diagnostic_id: string; os_code: number | null; operation: string } | null;
  configuration_revision: number | null;
}
