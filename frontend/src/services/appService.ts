import globalAxios from "@/api/modifiedAxios";

export async function restartApp() {
    const response = await globalAxios.post('/v1/app/restart');
    return response;
}