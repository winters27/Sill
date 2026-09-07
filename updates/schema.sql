-- One row per machine per day, and nothing that says which machine.
--
-- `id` is sha256(address + secret + day) cut to sixteen characters. The day is
-- inside the hash, so the same machine gets a different id tomorrow and no two
-- rows can be joined into a history of anybody. There is no table mapping an
-- id back to an address, here or anywhere else.
--
-- The primary key is the count: a machine checking forty times in a day is one
-- row, so `COUNT(*)` grouped by day is the number of machines that checked in.
CREATE TABLE IF NOT EXISTS checks (
  day TEXT NOT NULL,
  id TEXT NOT NULL,
  PRIMARY KEY (day, id)
);

-- The only question this is for.
--
--   SELECT day, COUNT(*) AS machines FROM checks GROUP BY day ORDER BY day DESC;
--
-- Retention is handled in the worker rather than here, so there is nothing
-- scheduled to forget about.
CREATE INDEX IF NOT EXISTS checks_by_day ON checks (day);
