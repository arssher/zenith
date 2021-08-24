import os
import timeit
from contextlib import closing
from fixtures.zenith_fixtures import PostgresFactory, ZenithPageserver

pytest_plugins = ("fixtures.zenith_fixtures", "fixtures.benchmark_fixture")

def get_timeline_size(repo_dir, tenantid, timelineid):
    path = "{}/tenants/{}/timelines/{}".format(repo_dir, tenantid, timelineid)

    totalbytes = 0
    for root, dirs, files in os.walk(path):
        for name in files:
            totalbytes += os.path.getsize(os.path.join(root, name))

        if 'wal' in dirs:
            dirs.remove('wal')  # don't visit 'wal' subdirectory

    return totalbytes

#
# Run a very short pgbench test.
#
# Collects three metrics:
#
# 1. Time to initialize the pgbench database (pgbench -s5 -i)
# 2. Time to run 5000 pgbench transactions
# 3. Disk space used
#
def test_pgbench(postgres: PostgresFactory, pageserver: ZenithPageserver, pg_bin, zenith_cli, zenbenchmark):
    # Create a branch for us
    zenith_cli.run(["branch", "test_pgbench_perf", "empty"])

    pg = postgres.create_start('test_pgbench_perf')
    print("postgres is running on 'test_pgbench_perf' branch")

    # Open a connection directly to the page server that we'll use to force
    # flushing the layers to disk
    psconn = pageserver.connect();
    pscur = psconn.cursor()

    # Get the timeline ID of our branch. We need it for the 'do_gc' command
    with closing(pg.connect()) as conn:
        with conn.cursor() as cur:
            cur.execute("SHOW zenith.zenith_timeline")
            timeline = cur.fetchone()[0]

    connstr = pg.connstr()

    start = timeit.default_timer()

    pg_bin.run_capture(['pgbench', '-s5', '-i', connstr])

    # Flush the layers from memory to disk. The time to do that is included in the
    # reported init time.
    pscur.execute(f"do_gc {pageserver.initial_tenant} {timeline} 0")
    end = timeit.default_timer()

    zenbenchmark.record('init', end - start, 's')

    # Run pgbench for 5000 transactions
    start = timeit.default_timer()
    pg_bin.run_capture(['pgbench', '-c1', '-t5000', connstr])
    end = timeit.default_timer()

    zenbenchmark.record('5000_xacts', end - start, 's')

    # Flush the layers to disk again. This is *not' included in the reported time,
    # though.
    pscur.execute(f"do_gc {pageserver.initial_tenant} {timeline} 0")

    # Report disk space used by the repository
    timeline_size = get_timeline_size(pageserver.repo_dir(), pageserver.initial_tenant, timeline)
    zenbenchmark.record('size', timeline_size / (1024*1024), 'MB')
