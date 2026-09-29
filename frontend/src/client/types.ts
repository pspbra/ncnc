/**
 * Type interfaces used by the application.
 * Extracted from the auto-generated OpenAPI client (api.ts) — only the types
 * actually consumed by components are kept here.
 */

export interface DownloadData {
    'id': string;
    'is_metadata': boolean;
    'name': string;
    'target_path': string;
    'dir': string;
    'total_length': number;
    'progress': number;
    'current_download': number;
    'status': string;
    'current_speed': number;
}

export interface Plugin {
    'namespace': string;
    'tasks': Array<ScheduledTask>;
}

export interface ScheduledTask {
    'id': number;
    'namespace': string;
    'name': string;
    'description': string;
    'cron_str': string | null;
}

export interface Setting {
    'key': string;
    'name': string;
    'description': string;
    'value': string | null;
}

export interface SettingRecord {
    'namespace': string;
    'settings': Array<Setting>;
}
