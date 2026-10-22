import type {
  CodingAgent,
  AgentProtocol,
  ServiceKind,
  ServiceHealth,
  ServiceManagement,
  LocalRuntimeComponent,
  LocalRuntimeState,
  LocalRuntimeErrorCode,
} from "/assets/shared/contracts.js";
import type { OutcomeService } from "./outcome-types.js";
export interface Service {
  id: string;
  kind: ServiceKind;
  endpoint: string;
  enabled: boolean;
  sandbox_id: string | null;
  management?: ServiceManagement;
}
export interface Adapter {
  id: string;
  agent: CodingAgent;
  protocol: AgentProtocol;
  enabled: boolean;
  executor_id: string;
  command: string[];
  env_names: string[];
  read_only_paths: string[];
  network: boolean;
  max_seconds: number;
}
export interface Configuration {
  revision: number;
  storage_id: string | null;
  services: Service[];
  adapters: Adapter[];
  bindings: Record<string, string>;
  binding_evidence?: Record<string, { request_id: string; verified_unix_ms: number; adapter_digest: string }>;
  outcome_services?: OutcomeService[];
  knowledge_groups?: Record<string, { service_id: string; repository: string }>;
  knowledge_goals?: Record<string, { service_id: string; repository: string }>;
}
export interface ServiceView {
  local_runtime?: { component: LocalRuntimeComponent; state: LocalRuntimeState; packaged: boolean; can_prepare: boolean; error: LocalRuntimeErrorCode | null; health: ServiceHealth | null }[];
  configuration: Configuration;
  health: {
    id: string;
    status: ServiceHealth;
    started?: boolean;
    info: { service_id: string; capabilities: string[] } | null;
    error: string | null;
  }[];
  restart_required: boolean;
  active_storage: [string, string] | null;
  protocol_version: number;
  outcome_health?: { id: string; status: ServiceHealth; error: string | null }[];
}
