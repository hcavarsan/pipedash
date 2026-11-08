FROM jenkins/jenkins:2.580.2-lts-jdk21@sha256:74401de6a6f92e1afe1b02d1c8e3d0c33f263bfb5275dd8813f9150355f2367b
RUN jenkins-plugin-cli --plugins configuration-as-code job-dsl workflow-aggregator cloudbees-folder matrix-auth
