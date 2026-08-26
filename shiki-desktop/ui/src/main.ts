import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

const app = document.getElementById("app");
if (!app) throw new Error("#app root element missing from index.html");

mount(App, { target: app });
