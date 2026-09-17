# Transport testbed

A local cluster of unmodified nwaku nodes for the transport measurements of
Section 6.6 of the paper: per-message wire framing, one-envelope versus
m-envelope burst packaging, and the maximum-envelope-size probe.

## Layout

    waku/node.sh              nwaku lifecycle: build | up N | message | stop | clean
    lib/common.sh             shared shell helpers
    experiments/packaging.py      framing constant + burst packaging comparison
    experiments/envelope_probe.py maximum message size probe (sweep + bisection)

## Usage

    bash waku/node.sh build      # one-time: builds nwaku v0.38.1 from source (~15 min)
    bash waku/node.sh up 5       # start a 5-node star-topology cluster, RLN off
    python3 experiments/packaging.py
    python3 experiments/envelope_probe.py
    bash waku/node.sh stop

Both experiments are dependency-free Python and write CSVs into `results/`.
Requirements: bash, curl, tar, python3; building nwaku additionally needs
git, make, gcc/g++, cargo, and about 2 GB of RAM.
