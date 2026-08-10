import { helper } from "./helper";
import type { Thing } from "./types";
import data from "./data.json";
import { Widget } from "./Widget";
import "./side-effect.js";
import React from "react";

const lazy = () => import("./lazy");

export { helper as helperAgain } from "./helper";

console.log(helper(data.value), Widget, React, lazy, /** @type {Thing | null} */ (null));
