import Vue from "vue";
import dayjs from "dayjs";

Vue.filter("formatDate", value => dayjs(value).format("YYYY-MM-DD HH:mm"));
