import globalAxios from "@/api/modifiedAxios";

export async function getSchedules() {
    const response = await globalAxios.get('/v1/schedule/all');
    return response;
}

export async function updateSchedule(scheduleId: number, cron_str: string | null) {
    const response = await globalAxios.post('/v1/schedule/update', {schedule_id: scheduleId, cron_str});
    return response;
}

export async function restartScheduler() {
    const response = await globalAxios.get('/v1/schedule/restart');
    return response;
}