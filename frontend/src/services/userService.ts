import globalAxios from "@/api/modifiedAxios";

export async function userLogin(username: string, password: string) {
    const response = await globalAxios.post('/v1/user/login', { username, password });
    return response;
}
