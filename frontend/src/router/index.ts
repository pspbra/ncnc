import { createRouter, createWebHistory } from "vue-router";
import { useUserStore } from "@/stores/user";
import MainView from "@/views/MainView.vue";



import pinia from "../stores"

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: "/login",
      name: "login",
      component: () => import("../views/LoginPage.vue"),
      meta: {
        title: "登录",
      },
    },
    {
      path: "/library",
      name: "library",
      component: MainView,
      children: [
        {
          name: "subscribe",
          path: "subscribe",
          component: () => import("../views/SubscribeView.vue"),
          meta: {
            title: "媒体库添加",
          },
        },
        {
          name: "movie-search",
          path: "movie/search",
          component: () => import("../views/MovieAddView.vue"),
          meta: {
            title: "电影添加",
          },
        },
        {
          name: "media",
          path: 'media',
          component: () => import("../views/MediaView.vue"),
          meta: {
            title: "媒体库",
          },
        },
        {
          name: "calendar",
          path: "calendar",
          component: () => import("../views/AirCalendarView.vue"),
          meta: { title: "播出日历" },
        },
        {
          name: "downloads",
          path: "downloads",
          component: () => import("../views/DownloadTasksView.vue"),
          meta: { title: "下载任务" },
        },
        {
          name: "logs",
          path: "logs",
          component: () => import("../views/LogView.vue"),
          meta: { title: "查看日志" },
        },
        {
          name: "media-detail",
          path: 'media/:tmdb_id',
          component: () => import("../views/MediaDetailView.vue"),
          meta: {
            title: "媒体详情",
          },
        },
        {
          name: "settings",
          path: "settings",
          component: () => import("../views/SettingsView.vue"),
          meta: {
            title: "设置",
          },
        },
        {
          path: '/:pathMatch(.*)*',
          redirect: '/library/media',
        }
      ]
    }
  ],
});

const userStore = useUserStore(pinia);

router.beforeEach((to, from, next) => {
  document.title = `${to.meta.title} - NCNC`;
  if (to.path === "/" || to.path === "/index.html") {
    next({ name: "media" });
    return;
  }
  if (to.name !== "login" && !userStore.isLogin) {
    next({ name: "login" });
  } else {
    next();
  }
});

export default router;
