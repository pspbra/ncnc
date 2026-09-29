import globalAxios from "@/api/modifiedAxios";

export async function getDownloads() {
    const response = await globalAxios.get('/v1/download/all');
    return response;
}

export async function clearAll() {
    const response = await globalAxios.post('/v1/download/clear');
    return response;
}