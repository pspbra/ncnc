import globalAxios from "@/api/modifiedAxios";

export async function getUploads() {
    const response = await globalAxios.get('/v1/upload/all');
    return response;
}
