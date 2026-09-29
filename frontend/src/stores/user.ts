import { defineStore } from "pinia";

export interface User {
  username: string;
}

export interface UserState {
  isLogin: boolean;
  user: User | null;
}

export const useUserStore = defineStore("user", {
  state: (): UserState => {
    const user = localStorage.getItem("user");
    if (user) {
      return {
        isLogin: true,
        user: JSON.parse(user),
      };
    } else {
      return {
        isLogin: false,
        user: null,
      };
    }
  },
  actions: {
    bindUser(username: string) {
      const user = {
        username,
      };
      this.isLogin = true;
      this.user = user;
      localStorage.setItem("user", JSON.stringify(user));
    },
    logout() {
      this.isLogin = false;
      this.user = null;
      localStorage.removeItem("user");
    },
  }
});
