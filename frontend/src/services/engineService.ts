import globalAxios from "@/api/modifiedAxios";

export async function runAllEngines() {
    const response = await globalAxios.get("/v1/update-all");
    return response.data;
}