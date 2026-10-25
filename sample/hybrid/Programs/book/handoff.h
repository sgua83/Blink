module book.handoff;
import simulation.statistics;
program finish(read dataset<BLTotal,1> left, read dataset<BLTotal,1> right,
    write dataset<BLTotal,1> combined) on compute;
