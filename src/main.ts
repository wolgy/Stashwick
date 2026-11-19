import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

document.documentElement.dataset.theme = 'auto'
const app = mount(App, { target: document.getElementById('app')! })

export default app
