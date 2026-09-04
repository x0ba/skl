import { sql } from "./index";
import { migrate } from "./migrate";

await migrate();
await sql.end({ timeout: 5 });
