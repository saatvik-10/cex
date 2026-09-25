import type { Config } from "./config";
import type { Db } from "../db/db";
import type { EngineQueue } from "../queue/queue";

export type AppContext = {
  config: Config;
  db: Db;
  queue: EngineQueue;
};
