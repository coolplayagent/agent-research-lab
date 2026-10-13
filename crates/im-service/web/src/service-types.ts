import type {
  CodingAgent,
  AgentProtocol,
  ServiceKind,
  ServiceHealth,
  ScenarioCapability,
} from "/assets/shared/contracts.js";
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
}
