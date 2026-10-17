import type {
  CodingAgent,
  AgentProtocol,
  ServiceKind,
  ServiceHealth,
  ScenarioCapability,
} from "/assets/shared/contracts.js";
import type { OutcomeService, OutcomeBinding } from "./outcome-types.js";
export interface Service {
  id: string;
  kind: ServiceKind;
  endpoint: string;
  enabled: boolean;
  sandbox_id: string | null;
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
  scenarios?: ScenarioRegistration[];
  outcome_services?: OutcomeService[];
  outcome_bindings?: Record<string, OutcomeBinding>;
  knowledge_groups?: Record<string, { service_id: string; repository: string }>;
  knowledge_goals?: Record<string, { service_id: string; repository: string }>;
}
export interface ScenarioRegistration {
  id: string;
  endpoint: string;
  enabled: boolean;
  protocol_version: number;
  capabilities: ScenarioCapability[];
  dependencies: string[];
}
export interface ServiceView {
  configuration: Configuration;
  health: {
    id: string;
    status: ServiceHealth;
    info: { service_id: string; capabilities: string[] } | null;
    error: string | null;
  }[];
  restart_required: boolean;
  active_storage: [string, string] | null;
  protocol_version: number;
  scenario_health?: {
    id: string;
    status: ServiceHealth;
    error: string | null;
  }[];
  outcome_health?: { id: string; status: ServiceHealth; error: string | null }[];
}
