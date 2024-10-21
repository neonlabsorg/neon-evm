import os
import re
import time

import docker
import sys
import subprocess
import requests
import json
import typing as tp
from urllib.parse import urlparse

from github_api_client import GithubClient

try:
    import click
except ImportError:
    print("Please install click library: pip install click==8.0.3")
    sys.exit(1)

ERR_MSG_TPL = {
    "blocks": [
        {
            "type": "section",
            "text": {"type": "mrkdwn", "text": ""},
        },
        {"type": "divider"},
    ]
}

DOCKER_USER = os.environ.get("DHUBU")
DOCKER_PASSWORD = os.environ.get("DHUBP")
IMAGE_NAME = os.environ.get("IMAGE_NAME", "evm_loader")
RUN_LINK_REPO = os.environ.get("RUN_LINK_REPO")
DOCKERHUB_ORG_NAME = os.environ.get("DOCKERHUB_ORG_NAME")
SOLANA_NODE_VERSION = 'v1.18.18'
SOLANA_BPF_VERSION = 'v1.18.18'

VERSION_BRANCH_TEMPLATE = r"[vt]{1}\d{1,2}\.\d{1,2}\.x.*"
RELEASE_TAG_TEMPLATE = r"[vt]{1}\d{1,2}\.\d{1,2}\.\d{1,2}"

docker_client = docker.APIClient()
NEON_TEST_IMAGE_NAME = "neon_tests"

PROXY_ENDPOINT = os.environ.get("PROXY_ENDPOINT")
NEON_TESTS_ENDPOINT = os.environ.get("NEON_TESTS_ENDPOINT")


@click.group()
def cli():
    pass


def ref_to_image_tag(ref):
    return ref.split('/')[-1]


def set_github_env(envs: tp.Dict, upper=True) -> None:
    """Set environment for github action"""
    path = os.getenv("GITHUB_ENV", str())
    if os.path.exists(path):
        print(f"Set environment variables: {envs}")
        with open(path, "a") as env_file:
            for key, value in envs.items():
                env_file.write(f"\n{key.upper() if upper else key}={str(value)}")


def is_image_exist(image, tag):
    response = requests.get(
        url=f"https://registry.hub.docker.com/v2/repositories/{DOCKERHUB_ORG_NAME}/{image}/tags/{tag}")
    return response.status_code == 200


@cli.command(name="specify_image_tags")
@click.option('--git_ref')
@click.option('--git_head_ref')
@click.option('--git_base_ref')
def specify_image_tags(git_ref,
                       git_head_ref,
                       git_base_ref):
    # evm_tag
    if "refs/pull" in git_ref:
        evm_tag = ref_to_image_tag(git_head_ref)
    elif git_ref == "refs/heads/develop":
        evm_tag = "latest"
    else:
        evm_tag = ref_to_image_tag(git_ref)

    # evm_pr_version_branch
    evm_pr_version_branch = ""
    if git_base_ref:
        if re.match(VERSION_BRANCH_TEMPLATE, ref_to_image_tag(git_base_ref)) is not None:
            evm_pr_version_branch = ref_to_image_tag(git_base_ref)

    # is_evm_release
    if "refs/tags/" in git_ref:
        is_evm_release = True
    else:
        is_evm_release = False

    # test_image_tag
    if evm_tag and is_image_exist(NEON_TEST_IMAGE_NAME, evm_tag):
        neon_test_tag = evm_tag
    elif is_evm_release:
        neon_test_tag = re.sub(r'\.[0-9]*$', '.x', evm_tag)
        if not is_image_exist(NEON_TEST_IMAGE_NAME, neon_test_tag):
            raise RuntimeError(f"{NEON_TEST_IMAGE_NAME} image with {neon_test_tag} tag isn't found")
    elif evm_pr_version_branch and is_image_exist(NEON_TEST_IMAGE_NAME, evm_pr_version_branch):
        neon_test_tag = evm_pr_version_branch
    else:
        neon_test_tag = "latest"

    env = dict(evm_tag=evm_tag,
               evm_pr_version_branch=evm_pr_version_branch,
               is_evm_release=is_evm_release,
               neon_test_tag=neon_test_tag)
    set_github_env(env)


@cli.command(name="build_docker_image")
@click.option('--evm_sha_tag')
def build_docker_image(evm_sha_tag):
    solana_image = f'solanalabs/solana:{SOLANA_NODE_VERSION}'
    docker_client.pull(solana_image)
    buildargs = {"REVISION": evm_sha_tag,
                 "SOLANA_IMAGE": solana_image,
                 "SOLANA_BPF_VERSION": SOLANA_BPF_VERSION,
                 "DOCKERHUB_ORG_NAME": DOCKERHUB_ORG_NAME}

    tag = f"{DOCKERHUB_ORG_NAME}/{IMAGE_NAME}:{evm_sha_tag}"
    click.echo("start build")
    output = docker_client.build(tag=tag, buildargs=buildargs, path="./", decode=True)
    process_output(output)


@cli.command(name="publish_image")
@click.option('--evm_sha_tag')
@click.option('--evm_tag')
def publish_image(evm_sha_tag, evm_tag):
    push_image_with_tag(evm_sha_tag, evm_sha_tag)
    # push latest and version tags only on the finalizing step
    if evm_tag != "latest" and re.match(RELEASE_TAG_TEMPLATE, evm_tag) is None:
        push_image_with_tag(evm_sha_tag, evm_tag)


@cli.command(name="finalize_image")
@click.option('--evm_sha_tag')
@click.option('--evm_tag')
def finalize_image(evm_sha_tag, evm_tag):
    if re.match(RELEASE_TAG_TEMPLATE, evm_tag) is not None or evm_tag == "latest":
        push_image_with_tag(evm_sha_tag, evm_tag)
    else:
        click.echo(f"Nothing to finalize, the tag {evm_tag} is not version tag or latest")


def push_image_with_tag(sha, tag):
    image = f"{DOCKERHUB_ORG_NAME}/{IMAGE_NAME}"
    docker_client.login(username=DOCKER_USER, password=DOCKER_PASSWORD)
    docker_client.tag(f"{image}:{sha}", f"{image}:{tag}")
    out = docker_client.push(f"{image}:{tag}", decode=True, stream=True)
    process_output(out)


def run_subprocess(command):
    click.echo(f"run command: {command}")
    subprocess.run(command, shell=True)


@cli.command(name="run_tests")
@click.option('--evm_sha_tag')
@click.option('--neon_test_tag')
@click.option('--run_number', default=1)
@click.option('--run_attempt', default=1)
def run_tests(evm_sha_tag, neon_test_tag, run_number, run_attempt):
    os.environ["EVM_LOADER_IMAGE"] = f"{DOCKERHUB_ORG_NAME}/{IMAGE_NAME}:{evm_sha_tag}"
    os.environ["NEON_TESTS_IMAGE"] = f"{DOCKERHUB_ORG_NAME}/{NEON_TEST_IMAGE_NAME}:{neon_test_tag}"
    project_name = f"neon-evm-{evm_sha_tag}-{run_number}-{run_attempt}"
    stop_containers(project_name)

    run_subprocess(f"docker-compose -p {project_name} -f ./ci/docker-compose-ci.yml pull")
    run_subprocess(f"docker-compose -p {project_name} -f ./ci/docker-compose-ci.yml up -d")
    test_container_name = get_container_name(project_name, "tests")

    click.echo("Start tests")
    print(test_container_name)
    exec_id = docker_client.exec_create(
        container=test_container_name, cmd="python3 clickfile.py run evm --numprocesses 8 --network docker_net")
    logs = docker_client.exec_start(exec_id['Id'], stream=True)

    tests_are_failed = False
    all_logs = ""
    for line in logs:
        current_line = line.decode('utf-8')
        all_logs += current_line
        click.echo(current_line)
        if 'ERROR ' in current_line or 'FAILED ' in current_line or 'Error: ' in current_line:
            tests_are_failed = True
            print("Tests are failed")

    exec_status = docker_client.exec_inspect(exec_id['Id'])["ExitCode"]

    run_subprocess(f"docker-compose -p {project_name} -f ./ci/docker-compose-ci.yml logs neon-core-api")
    run_subprocess(f"docker cp {test_container_name}:/opt/neon-tests/allure-results ./allure-results")

    stop_containers(project_name)

    if tests_are_failed or exec_status == 1:
        sys.exit(1)


def get_container_name(project_name, service_name):
    data = subprocess.run(
        f"docker-compose -p {project_name} -f ./ci/docker-compose-ci.yml ps",
        shell=True, capture_output=True, text=True).stdout
    click.echo(data)
    pattern = rf'{project_name}[-_]{service_name}[-_]1'
    match = re.search(pattern, data)
    return match.group(0)


def stop_containers(project_name):
    run_subprocess(f"docker-compose -p {project_name} -f ./ci/docker-compose-ci.yml down")


@cli.command(name="trigger_proxy_action")
@click.option('--evm_pr_version_branch')
@click.option('--is_evm_release')
@click.option('--evm_sha_tag')
@click.option('--evm_tag')
@click.option('--token')
@click.option('--labels')
@click.option('--pr_url')
@click.option('--pr_number')
def trigger_proxy_action(evm_pr_version_branch, is_evm_release, evm_sha_tag, evm_tag, token, labels,
                         pr_url, pr_number):
    is_version_branch = re.match(VERSION_BRANCH_TEMPLATE, evm_tag) is not None
    is_FTS_labeled = 'fullTestSuit' in labels

    if evm_tag == "latest" or is_evm_release == 'True' or is_version_branch or is_FTS_labeled:
        full_test_suite = True
    else:
        full_test_suite = False

    github = GithubClient(token)

    # get proxy branch by evm_tag
    if GithubClient.is_branch_exist(PROXY_ENDPOINT, evm_tag):
        proxy_branch = evm_tag
    elif evm_pr_version_branch:
        proxy_branch = evm_pr_version_branch
    elif is_evm_release == 'True':
        proxy_branch = re.sub(r'\.\d+$', '.x', evm_tag)
    elif is_version_branch:
        proxy_branch = evm_tag
    else:
        proxy_branch = 'develop'
    click.echo(f"Proxy branch: {proxy_branch}")

    initial_pr = f"{pr_url}/{pr_number}/comments" if pr_number else ""

    runs_before = github.get_proxy_runs_list(proxy_branch)
    runs_count_before = github.get_proxy_runs_count(proxy_branch)
    github.run_proxy_dispatches(proxy_branch, evm_tag, evm_sha_tag, evm_pr_version_branch, full_test_suite, initial_pr)
    wait_condition(lambda: github.get_proxy_runs_count(proxy_branch) > runs_count_before)

    runs_after = github.get_proxy_runs_list(proxy_branch)
    proxy_run_id = list(set(runs_after) - set(runs_before))[0]
    link = f"https://github.com/{RUN_LINK_REPO}/actions/runs/{proxy_run_id}"
    click.echo(f"Proxy run link: {link}")
    click.echo("Waiting for completed status...")
    wait_condition(lambda: github.get_proxy_run_info(proxy_run_id)["status"] == "completed", timeout_sec=10800, delay=5)

    if github.get_proxy_run_info(proxy_run_id)["conclusion"] == "success":
        click.echo("Proxy tests passed successfully")
    else:
        raise RuntimeError(f"Proxy tests failed! See {link}")


def wait_condition(func_cond, timeout_sec=60, delay=0.5):
    start_time = time.time()
    while True:
        if time.time() - start_time > timeout_sec:
            raise RuntimeError(f"The condition not reached within {timeout_sec} sec")
        try:
            if func_cond():
                break
        except:
            raise
        time.sleep(delay)


@cli.command(name="send_notification", help="Send notification to slack")
@click.option("--evm_tag", help="slack app endpoint url.")
@click.option("--url", help="slack app endpoint url.")
@click.option("--build_url", help="github action test build url.")
def send_notification(evm_tag, url, build_url):

    if re.match(RELEASE_TAG_TEMPLATE, evm_tag) is not None \
        or re.match(VERSION_BRANCH_TEMPLATE, evm_tag) is not None \
            or evm_tag == "latest":
        tpl = ERR_MSG_TPL.copy()

        parsed_build_url = urlparse(build_url).path.split("/")
        build_id = parsed_build_url[-1]
        repo_name = f"{parsed_build_url[1]}/{parsed_build_url[2]}"

        tpl["blocks"][0]["text"]["text"] = (
            f"*Build <{build_url}|`{build_id}`> of repository `{repo_name}` is failed.*"
            f"\n<{build_url}|View build details>"
        )
        requests.post(url=url, data=json.dumps(tpl))
    else:
        click.echo(f"Notification is not sent, the tag {evm_tag} is not version tag or latest")


def process_output(output):
    for line in output:
        if line:
            errors = set()
            try:
                if "status" in line:
                    click.echo(line["status"])

                elif "stream" in line:
                    stream = re.sub("^\n", "", line["stream"])
                    stream = re.sub("\n$", "", stream)
                    stream = re.sub("\n(\x1B\[0m)$", "\\1", stream)
                    if stream:
                        click.echo(stream)

                elif "aux" in line:
                    if "Digest" in line["aux"]:
                        click.echo("digest: {}".format(line["aux"]["Digest"]))

                    if "ID" in line["aux"]:
                        click.echo("ID: {}".format(line["aux"]["ID"]))

                else:
                    click.echo("not recognized (1): {}".format(line))

                if "error" in line:
                    errors.add(line["error"])

                if "errorDetail" in line:
                    errors.add(line["errorDetail"]["message"])

                    if "code" in line:
                        error_code = line["errorDetail"]["code"]
                        errors.add("Error code: {}".format(error_code))

            except ValueError as e:
                click.echo("not recognized (2): {}".format(line))

            if errors:
                message = "problem executing Docker: {}".format(". ".join(errors))
                raise SystemError(message)

@cli.group("allure")
@click.pass_context
def allure_cli(ctx):
    """Commands for load test manipulation."""


# @allure_cli.command("get-history", help="Download allure history")
# @click.argument("name", type=click.STRING)
# @click.option("-n", "--network", default="night-stand", type=str, help="In which stand run tests")
# @click.option(
#     "-d",
#     "--destination",
#     default="./allure-results",
#     type=click.Path(file_okay=False, dir_okay=True),
# )
# def get_allure_history(name: str, network: str, destination: str = "./allure-results"):
#     branch = os.environ.get("GITHUB_REF_NAME")
#     path = Path(name) / network / branch

#     runs = []
#     previous_runs = cloud.client.list_objects_v2(
#         Bucket=cloud.NEON_TESTS_BUCKET_NAME, Prefix=f"{path}/", Delimiter="/"
#     ).get("CommonPrefixes", [])
#     for run in previous_runs:
#         run_id = re.findall(r"(\d+)", run["Prefix"])
#         if len(run_id) > 0:
#             runs.append(int(run_id[0]))
#     if len(runs) > 0:
#         print(f"Downloading allure history from build: {max(runs)}")
#         cloud.download(path / str(max(runs)) / "history", Path(destination) / "history")


# @allure_cli.command("upload-report", help="Upload allure history")
# @click.argument("name", type=click.Choice(TEST_GROUPS))
# @click.option("-n", "--network", default=EnvName.NIGHT_STAND, type=EnvName, help="In which stand run tests")
# @click.option(
#     "-s",
#     "--source",
#     default="./allure-report",
#     type=click.Path(file_okay=False, dir_okay=True),
# )
# def upload_allure_report(name: TestGroup, network: EnvName, source: str = "./allure-report"):
#     branch = os.environ.get("GITHUB_REF_NAME")
#     build_id = os.environ.get("GITHUB_RUN_NUMBER")
#     path = Path(name) / network.value / branch
#     cloud.upload(source, path / build_id)
#     report_url = f"http://neon-test-allure.s3-website.eu-central-1.amazonaws.com/{path / build_id}"

#     with open(ALLURE_REPORT_URL, "w") as f:
#         f.write(report_url)

#     with open("/tmp/index.html", "w") as f:
#         f.write(
#             f"""<!DOCTYPE html><meta charset="utf-8"><meta http-equiv="refresh" content="0; URL={report_url}">
#         <meta http-equiv="Pragma" content="no-cache"><meta http-equiv="Expires" content="0">
#         """
#         )

#     cloud.upload("/tmp/index.html", path)
#     print(f"Allure report link: {report_url}")

#     with open("allure_report_info", "w") as f:
#         f.write(f"🔗 Allure [report]({report_url})\n")


@allure_cli.command("generate", help="Generate allure history")
def generate_allure_report():
    cmd = subprocess.run("allure generate", shell=True)
    if cmd.returncode != 0:
        sys.exit(cmd.returncode)

if __name__ == "__main__":
    cli()
