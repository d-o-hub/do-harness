-- 0005_beat_sensor.sql
-- Record the sensor name on sensor beats so the task advance gate can verify
-- that the specific sensor (not merely any sensor) passed for a task.

ALTER TABLE beats ADD COLUMN sensor_name TEXT;
