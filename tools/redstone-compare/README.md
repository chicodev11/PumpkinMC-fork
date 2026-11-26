# Redstone timing comparison

Builds the same redstone contraptions on a vanilla server and on Pumpkin. Both servers advance one tick at a time, and each run records the block states at fixed probe positions every tick, the positions of tagged entities (`entities` in a scenario) for contraptions that move them, and how many entities of a type are in the area (`counts`, written `type#n`) for TNT dupers and farms. You then diff the two traces.

Commands run between ticks on both servers: over RCON while vanilla is frozen with `/tick freeze`, and from the console between `server.tick()` calls in the Pumpkin tester. Run both servers at the same coordinates, because the order in which redstone wire updates its neighbours depends on the position (`HashSet` order).

## Run

1. Run `python3 scenarios.py`. It writes `scenarios.json`.
2. Start a vanilla server. Its version must match Pumpkin's data version, and 26.3 needs Java 25. Set these in `server.properties`:

   ```
   enable-rcon=true
   rcon.password=cmp
   rcon.port=25575
   online-mode=false
   pause-when-empty-seconds=-1
   ```

   Without `pause-when-empty-seconds=-1`, the server pauses one minute after it starts empty, and `/tick step` stops advancing.
   Keep the server's stdin open (for example run it from a terminal); a server whose stdin is closed stops right after it starts.
3. Run `python3 vanilla_run.py <this dir>`. It writes `vanilla_trace.txt`.
4. Run Pumpkin:

   ```
   REDSTONE_COMPARE=<this dir>/scenarios.json TRACE_OUT=<this dir>/pumpkin_trace.txt \
     cargo run -p pumpkin-tester
   ```

5. Run `diff vanilla_trace.txt pumpkin_trace.txt`.

Keep the contraptions inside the cleared `AREA`. Blocks outside it come from world generation, which differs between the two servers; for example, slime blocks pull the stone floor along with them.
