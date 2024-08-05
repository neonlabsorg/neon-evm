import click
import requests
import os


class GithubClient():

    def __init__(self, token):
        self.proxy_endpoint = os.environ.get("PROXY_ENDPOINT")
        self.headers = {"Authorization": f"Bearer {token}",
                        "Accept": "application/vnd.github+json"}

    def get_proxy_runs_list(self, proxy_branch):
        response = requests.get(
            f"{self.proxy_endpoint}/actions/workflows/pipeline.yml/runs?branch={proxy_branch}", headers=self.headers)
        if response.status_code != 200:
            raise RuntimeError(f"Can't get proxy runs list. Error: {response.json()}")
        runs = [item['id'] for item in response.json()['workflow_runs']]
        return runs

    def get_proxy_runs_count(self, proxy_branch):
        response = requests.get(
            f"{self.proxy_endpoint}/actions/workflows/pipeline.yml/runs?branch={proxy_branch}", headers=self.headers)
        return int(response.json()["total_count"])

    def run_proxy_dispatches(
            self,
            proxy_branch: str,
            neon_evm_branch: str,
            github_sha: str,
            full_test_suite: bool,
            initial_pr: str,
            last_commit_message: str,
    ):
        neon_evm_github_event_name = os.getenv('GITHUB_EVENT_NAME', '')
        neon_evm_github_ref = os.getenv('GITHUB_REF', '')
        neon_evm_github_ref_name = os.getenv('GITHUB_REF_NAME', '')
        neon_evm_github_head_ref = os.getenv('GITHUB_HEAD_REF', '')
        neon_evm_github_base_ref = os.getenv('GITHUB_BASE_REF', '')

        # Construct the data dictionary
        data = {
            "ref": proxy_branch,
            "inputs": {
                "full_test_suite": f"{full_test_suite}".lower(),
                "neon_evm_commit": github_sha,
                "neon_evm_branch": neon_evm_branch,
                "initial_pr": initial_pr,
                "neon_evm_github_event_name": neon_evm_github_event_name,
                "neon_evm_github_ref": neon_evm_github_ref,
                "neon_evm_github_ref_name": neon_evm_github_ref_name,
                "neon_evm_github_head_ref": neon_evm_github_head_ref,
                "neon_evm_github_base_ref": neon_evm_github_base_ref,
                "neon_evm_last_commit_message": last_commit_message,
            }
        }
        response = requests.post(
            f"{self.proxy_endpoint}/actions/workflows/pipeline.yml/dispatches", json=data, headers=self.headers)
        click.echo(f"Sent data: {data}")
        click.echo(f"Status code: {response.status_code}")
        if response.status_code != 204:
            raise RuntimeError("Proxy action is not triggered, error: {response.text}")

    @staticmethod
    def is_branch_exist(endpoint, branch):
        if branch:
            response = requests.get(f"{endpoint}/branches/{branch}")
            if response.status_code == 200:
                click.echo(f"The branch {branch} exist in the {endpoint} repository")
                return True
        else:
            return False

    def get_proxy_run_info(self, id):
        response = requests.get(
            f"{self.proxy_endpoint}/actions/runs/{id}", headers=self.headers)
        return response.json()
