import axios from 'axios';
import { useUserStore } from '@/stores/user'
import { useMessageStore } from '@/stores/message';
import pinia from '../stores'


const userStore = useUserStore(pinia);
const messageStore = useMessageStore(pinia);

let baseURL = '/';
if (import.meta.env.DEV) {
    baseURL = 'http://localhost:8000';
}

const globalAxios = axios.create({
    baseURL: baseURL,
    withCredentials: true,
});

globalAxios.interceptors.response.use(
    response => {
        return response;
    },
    error => {
        if (error.response && error.response.status === 401) {
            messageStore.setMessage('登录失效~请重新登录QAQ', 'error');
            userStore.logout();
            window.location.href = '/webui/login';
        }
        return Promise.reject(error);
    }
);

export default globalAxios;
