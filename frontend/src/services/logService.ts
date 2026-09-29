import globalAxios from "@/api/modifiedAxios";

export interface LogDatesResponse {
  success: boolean;
  message: string | null;
  data: string[];
}

export interface LogContentResponse {
  success: boolean;
  message: string | null;
  data: string | null;
}

export async function getLogDates() {
  return globalAxios.get<LogDatesResponse>("/v1/log/dates");
}

export async function getLogContent(date: string) {
  return globalAxios.get<LogContentResponse>("/v1/log/content", {
    params: { date },
  });
}
